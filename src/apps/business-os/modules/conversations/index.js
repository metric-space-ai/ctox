import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';

// This module reads the canonical CTOX channel projection (mirror of the
// communication_* tables in runtime/ctox.sqlite3). Per-channel threads are the
// source-of-truth shape; the contact-centric list and channel-tabbed detail
// view bucket them client-side by participant_keys_json.

const STYLE_BUILD = '20260605-rxdb-cancel1';
const SUPPORTED_CHANNELS = ['whatsapp', 'email', 'jami', 'teams', 'meeting'];
const COMMUNICATION_DIAGNOSTIC_COLLECTIONS = [
  'communication_accounts',
  'communication_threads',
  'communication_messages',
];
const OUTBOUND_CONTEXT_COLLECTIONS = [
  'outbound_campaigns',
  'outbound_pipeline_items',
  'outbound_engagements',
  'outbound_messages',
  'outbound_approvals',
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
  jami: 'Jami',
  teams: 'MS Teams',
  meeting: 'Meeting',
};
const ALL_CHANNELS_TAB = '__all__';
const MESSAGE_PAGE_SIZE = 200;
const DAY_MS = 86400000;
const ACCOUNT_HEALTH_WARN_MS = 24 * 60 * 60 * 1000;
const ACCOUNT_HEALTH_BAD_MS = 7 * 24 * 60 * 60 * 1000;

const FALLBACK_LABELS = {
  de: {
    moduleTitle: 'Conversations',
    moduleSubtitle: 'Audit aus CTOX-Sicht',
    channelAll: 'Alle Channels',
    channelWhatsapp: 'WhatsApp',
    channelEmail: 'E-Mail',
    channelJami: 'Jami',
    channelTeams: 'MS Teams',
    channelMeeting: 'Meeting',
    searchPlaceholder: 'Kontakt/Text/Account',
    emptyListTitle: 'Keine Konversationen',
    emptyListBody: 'Hier erscheinen Kommunikationen, die CTOX über WhatsApp, E-Mail, Jami, MS Teams oder Meeting führt.',
    noResultsTitle: 'Keine Treffer',
    noResultsBody: 'Keine Konversation passt zu den aktiven Filtern.',
    syncFailureTitle: 'Kommunikation ist gerade nicht verfügbar',
    syncFailureBody: 'Konversationen erscheinen automatisch, sobald Kommunikationsdaten geladen sind.',
    syncStartingTitle: 'Kommunikation wird synchronisiert',
    syncStartingBody: 'Accounts und Nachrichten sind noch nicht vollständig in der lokalen App angekommen.',
    projectionMissingTitle: 'Konversationen werden vorbereitet',
    projectionMissingBody: 'Accounts und Nachrichten werden gerade für die Kontaktansicht vorbereitet.',
    diagnosticsLabel: 'Status',
    accountFilterEmpty: 'Keine Accounts verfügbar',
    accountFilterCount: 'Accounts verfügbar',
    accountFilterSyncFailure: 'Accounts gerade nicht verfügbar',
    emptyDetailTitle: 'Keine Konversation ausgewählt',
    emptyDetailBody: 'Wähle links eine Konversation, um die Timeline aus CTOX-Sicht zu sehen.',
    rightEmptyBody: 'Kontaktdaten und verknüpfte Datensätze erscheinen hier.',
    cardLabelContact: 'Kontakt',
    cardLabelChannels: 'Aktive Channels',
    cardLabelOutbound: 'Outbound',
    cardLabelStats: 'Aktivität',
    cardLabelAccount: 'CTOX-Account',
    cardLabelAccounts: 'CTOX-Accounts',
    cardLabelFolder: 'E-Mail-Folder',
    statMessages: 'Nachrichten',
    statFirst: 'Erste',
    statLast: 'Letzte',
    statAccount: 'CTOX-Account',
    statTrust: 'Trust',
    directionInbound: 'Eingang',
    directionOutbound: 'Ausgang',
    directionAny: 'Alle Richtungen',
    dateAny: 'Alle Zeiträume',
    dateToday: 'Heute',
    date7d: 'Letzte 7 Tage',
    date30d: 'Letzte 30 Tage',
    accountAll: 'Alle Accounts',
    statusSent: 'gesendet',
    statusDelivered: 'zugestellt',
    statusRead: 'gelesen',
    statusReceived: 'eingegangen',
    statusFailed: 'fehlgeschlagen',
    statusQueued: 'in Warteschlange',
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
    contextCopyId: 'Message-Key kopieren',
    contextCopyBody: 'Inhalt kopieren',
    contextCopyRemoteId: 'Remote-ID kopieren',
    contextCopyWorkId: 'Work-ID kopieren',
    contextOpenAttachment: 'Anhang öffnen',
    contextOpenFlowview: 'Im CTOX-Flow öffnen',
    taskBadgeLabel: 'Task',
    workBadgeLabel: 'Work',
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
    copiedToast: 'In Zwischenablage kopiert',
    outboundOpenCampaign: 'In Outbound öffnen',
    outboundApprove: 'Freigeben',
    outboundReject: 'Ablehnen',
    outboundAwaitingApproval: 'wartet auf Freigabe',
    outboundReplyLabel: 'Antwort',
    outboundReplyClassify: 'Klassifizieren',
    outboundReplyClassification: 'Reply-Klasse',
    outboundReplyMatched: 'Antwort automatisch zugeordnet',
  },
  en: {
    moduleTitle: 'Conversations',
    moduleSubtitle: 'Audit from CTOX’s perspective',
    channelAll: 'All channels',
    channelWhatsapp: 'WhatsApp',
    channelEmail: 'Email',
    channelJami: 'Jami',
    channelTeams: 'MS Teams',
    channelMeeting: 'Meeting',
    searchPlaceholder: 'Contact/text/account',
    emptyListTitle: 'No conversations',
    emptyListBody: 'Communications CTOX has on WhatsApp, Email, Jami, MS Teams, or Meeting appear here.',
    noResultsTitle: 'No matches',
    noResultsBody: 'No conversation matches the active filters.',
    syncFailureTitle: 'Communication is unavailable right now',
    syncFailureBody: 'Conversations appear automatically once communication data is loaded.',
    syncStartingTitle: 'Communication is syncing',
    syncStartingBody: 'Accounts and messages have not fully arrived in the local app yet.',
    projectionMissingTitle: 'Preparing conversations',
    projectionMissingBody: 'Accounts and messages are being prepared for the contact view.',
    diagnosticsLabel: 'Status',
    accountFilterEmpty: 'No accounts available',
    accountFilterCount: 'accounts available',
    accountFilterSyncFailure: 'Accounts unavailable right now',
    emptyDetailTitle: 'No conversation selected',
    emptyDetailBody: 'Pick a conversation on the left to view the CTOX-side timeline.',
    rightEmptyBody: 'Contact details and linked records appear here.',
    cardLabelContact: 'Contact',
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
    contextCopyId: 'Copy message key',
    contextCopyBody: 'Copy body',
    contextCopyRemoteId: 'Copy remote ID',
    contextCopyWorkId: 'Copy work ID',
    contextOpenAttachment: 'Open attachment',
    contextOpenFlowview: 'Open in CTOX flow',
    taskBadgeLabel: 'Task',
    workBadgeLabel: 'Work',
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
    copiedToast: 'Copied to clipboard',
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

export async function mount(ctx) {
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;

  const root = ctx.host.querySelector('[data-conv-root]');
  if (!root) throw new Error('conversations: root missing after fragment mount');

  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  // Standardised 3-column resizer system
  const leftResizerEl = ctx.host.querySelector('[data-resizer="left"]');
  const rightResizerEl = ctx.host.querySelector('[data-resizer="right"]');

  const leftStorageKey = 'ctox.conversations.layout.leftWidth';
  const rightStorageKey = 'ctox.conversations.layout.rightWidth';

  const savedLeftWidth = localStorage.getItem(leftStorageKey) || '320';
  const savedRightWidth = localStorage.getItem(rightStorageKey) || '320';

  root.style.setProperty('--conversations-left-width', `${savedLeftWidth}px`);
  root.style.setProperty('--conversations-right-width', `${savedRightWidth}px`);

  // Column resizing is now owned by the shell-global resizer (setupModuleResizers
  // in app.js), which wires the `.ctox-column-resizer[data-resizer-var]` handles in
  // index.html declaratively (drag + keyboard + per-module localStorage). We must
  // NOT DIY-wire them here or each handle gets double-wired. Leave the references in
  // place (null) so the unmount cleanup below stays valid.
  const resizerL = null;
  const resizerR = null;
  void leftResizerEl;
  void rightResizerEl;
  void CtoxResizer;

  const messages = await loadModuleMessages(import.meta.url, ctx.locale, FALLBACK_LABELS);
  const t = (key, fallback) => messages[key] ?? fallback ?? key;

  applyStaticLabels(root, t);

  const refs = collectRefs(root);
  const threadsCollection = ctx.db?.collection?.('communication_threads') || null;
  const messagesCollection = ctx.db?.collection?.('communication_messages') || null;
  const accountsCollection = ctx.db?.collection?.('communication_accounts') || null;
  const outboundCampaignsCollection = ctx.db?.collection?.('outbound_campaigns') || null;
  const outboundPipelineItemsCollection = ctx.db?.collection?.('outbound_pipeline_items') || null;
  const outboundEngagementsCollection = ctx.db?.collection?.('outbound_engagements') || null;
  const outboundMessagesCollection = ctx.db?.collection?.('outbound_messages') || null;
  const outboundApprovalsCollection = ctx.db?.collection?.('outbound_approvals') || null;
  const businessCommandsCollection = ctx.db?.collection?.('business_commands') || null;

  const view = {
    channel: 'all',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
    threads: [],
    accountsById: new Map(),
    messageProbeCount: null,
    collectionErrors: new Map(),
    missingCollections: new Set(),
    syncDiagnostics: ctx.sync?.diagnostics || window.ctoxBusinessOsSyncDiagnostics || null,
    buckets: [],
    selectedBucketKey: null,
    selectedChannel: ALL_CHANNELS_TAB,
    timelineMessages: [],
    timelineTotal: 0,
    timelineLimit: MESSAGE_PAGE_SIZE,
    deepLink: parseConversationDeepLink(),
    highlightedMessageKey: '',
    outboundContext: null,
    outboundContextRetryTimer: null,
    outboundStatus: '',
    outboundReplyMatchKeys: new Set(),
  };
  view.highlightedMessageKey = view.deepLink.message_key || '';

  const cleanups = [];
  if (resizerL) cleanups.push(() => resizerL.destroy());
  if (resizerR) cleanups.push(() => resizerR.destroy());
  cleanups.push(() => {
    if (view.outboundContextRetryTimer) window.clearTimeout(view.outboundContextRetryTimer);
    view.outboundContextRetryTimer = null;
  });

  markMissingCollections();
  ensureCommunicationCollectionsSync();

  wireChannelFilters(refs.channelFilterButtons, view, renderList);
  refs.search.addEventListener('input', () => {
    view.search = refs.search.value.trim().toLowerCase();
    renderList();
  });
  refs.accountFilter.addEventListener('change', () => {
    view.account = refs.accountFilter.value;
    renderList();
  });
  refs.directionFilter.addEventListener('change', () => {
    view.direction = refs.directionFilter.value;
    renderList();
    if (view.selectedBucketKey) refreshTimelineForActiveBucket();
  });
  refs.dateFilter.addEventListener('change', () => {
    view.dateRange = refs.dateFilter.value;
    renderList();
    if (view.selectedBucketKey) refreshTimelineForActiveBucket();
  });

  await Promise.all([loadAccounts(), loadThreads(), probeMessages()]);
  rebuildBuckets();
  populateAccountFilter();
  await applyConversationDeepLink();
  renderList();

  if (threadsCollection?.$) {
    const sub = threadsCollection.$.subscribe(() => {
      loadThreads().then(() => {
        rebuildBuckets();
        populateAccountFilter();
        renderList();
        if (view.selectedBucketKey) renderDetail();
      }).catch((error) => console.error('[conversations] thread refresh failed:', error));
    });
    cleanups.push(() => sub.unsubscribe?.());
  }

  if (messagesCollection?.$) {
    const sub = messagesCollection.$.subscribe((change) => {
      probeMessages().then(() => renderList()).catch(() => {});
      const doc = change?.documentData || change?.doc?._data || change?.doc;
      if (!doc) return;
      const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
      if (!bucket) return;
      if (!bucket.threadKeys.has(doc.thread_key)) return;
      refreshTimelineForActiveBucket();
    });
    cleanups.push(() => sub.unsubscribe?.());
  }

  if (accountsCollection?.$) {
    const sub = accountsCollection.$.subscribe(() => {
      loadAccounts().then(() => {
        populateAccountFilter();
        if (view.selectedBucketKey) renderRightPane();
      }).catch((error) => console.error('[conversations] account refresh failed:', error));
    });
    cleanups.push(() => sub.unsubscribe?.());
  }

  const syncDiagnosticsHandler = (event) => {
    view.syncDiagnostics = event.detail || window.ctoxBusinessOsSyncDiagnostics || ctx.sync?.diagnostics || null;
    populateAccountFilter();
    renderList();
    if (view.selectedBucketKey) renderRightPane();
    else renderRightPane();
  };
  window.addEventListener('ctox-business-os-sync-diagnostics', syncDiagnosticsHandler);
  cleanups.push(() => window.removeEventListener('ctox-business-os-sync-diagnostics', syncDiagnosticsHandler));

  const hashChangeHandler = async () => {
    if (!String(location.hash || '').startsWith('#conversations')) return;
    view.deepLink = parseConversationDeepLink();
    view.highlightedMessageKey = view.deepLink.message_key || '';
    await applyConversationDeepLink();
    renderList();
    if (view.selectedBucketKey) await renderDetail();
  };
  window.addEventListener('hashchange', hashChangeHandler);
  cleanups.push(() => window.removeEventListener('hashchange', hashChangeHandler));

  return () => {
    for (const dispose of cleanups) {
      try { dispose?.(); } catch (error) { console.error('[conversations] cleanup:', error); }
    }
  };

  // ----- data loaders -----

  async function loadThreads() {
    if (!threadsCollection) {
      view.threads = [];
      view.missingCollections.add('communication_threads');
      return;
    }
    try {
      const docs = await threadsCollection.find().exec();
      view.threads = docs
        .map((doc) => doc.toJSON())
        .sort((a, b) => compareIsoDesc(a.last_message_at, b.last_message_at));
      view.collectionErrors.delete('communication_threads');
      view.missingCollections.delete('communication_threads');
    } catch (error) {
      console.error('[conversations] loadThreads failed:', error);
      view.collectionErrors.set('communication_threads', error);
      view.threads = [];
    }
  }

  async function loadAccounts() {
    if (!accountsCollection) {
      view.accountsById = new Map();
      view.missingCollections.add('communication_accounts');
      return;
    }
    try {
      const docs = await accountsCollection.find().exec();
      view.accountsById = new Map(
        docs
          .map((doc) => doc.toJSON())
          .filter((account) => account && account._deleted !== true && account.is_deleted !== true)
          .map((account) => [account.account_key, account]),
      );
      view.collectionErrors.delete('communication_accounts');
      view.missingCollections.delete('communication_accounts');
    } catch (error) {
      console.error('[conversations] loadAccounts failed:', error);
      view.collectionErrors.set('communication_accounts', error);
      view.accountsById = new Map();
    }
  }

  async function probeMessages() {
    if (!messagesCollection) {
      view.messageProbeCount = null;
      view.missingCollections.add('communication_messages');
      return;
    }
    try {
      const docs = await messagesCollection.find({ limit: 1 }).exec();
      view.messageProbeCount = docs
        .map((doc) => doc.toJSON?.() || doc)
        .filter((message) => message && message._deleted !== true && message.is_deleted !== true)
        .length;
      view.collectionErrors.delete('communication_messages');
      view.missingCollections.delete('communication_messages');
    } catch (error) {
      console.error('[conversations] probeMessages failed:', error);
      view.collectionErrors.set('communication_messages', error);
      view.messageProbeCount = null;
    }
  }

  async function loadTimelineForBucket(bucket) {
    if (!messagesCollection || !bucket) {
      view.timelineMessages = [];
      view.timelineTotal = 0;
      return view.timelineMessages;
    }
    try {
      const threadKeys = [...bucket.threadKeys];
      const docs = await messagesCollection
        .find({ selector: { thread_key: { $in: threadKeys } } })
        .exec();
      let all = docs.map((doc) => doc.toJSON());
      all = applyMessageFilters(all);
      all.sort((a, b) => compareIsoDesc(a.external_created_at, b.external_created_at));
      view.timelineTotal = all.length;
      const trimmed = all.slice(0, view.timelineLimit);
      trimmed.reverse();
      view.timelineMessages = trimmed;
      view.collectionErrors.delete('communication_messages');
    } catch (error) {
      console.error('[conversations] loadTimeline failed:', error);
      view.collectionErrors.set('communication_messages', error);
      view.timelineMessages = [];
      view.timelineTotal = 0;
    }
    return view.timelineMessages;
  }

  function markMissingCollections() {
    const required = ['communication_accounts', 'communication_threads', 'communication_messages'];
    for (const name of required) {
      if (!ctx.db?.collection?.(name)) view.missingCollections.add(name);
      else view.missingCollections.delete(name);
    }
  }

  function ensureCommunicationCollectionsSync() {
    for (const name of COMMUNICATION_DIAGNOSTIC_COLLECTIONS) {
      ctx.sync?.startCollection?.(name)?.catch?.((error) => {
        console.warn(`[conversations] ${name} sync start failed`, error);
        view.collectionErrors.set(name, error);
        populateAccountFilter();
        renderList();
      });
    }
  }

  function ensureOutboundContextCollectionsSync() {
    for (const name of OUTBOUND_CONTEXT_COLLECTIONS) {
      if (!ctx.db?.collection?.(name)) continue;
      ctx.sync?.startCollection?.(name)?.catch?.((error) => {
        console.warn(`[conversations] ${name} outbound context sync start failed`, error);
        view.collectionErrors.set(name, error);
      });
    }
  }

  function applyMessageFilters(messages) {
    const cutoff = dateCutoff(view.dateRange);
    return messages.filter((msg) => {
      if (view.selectedChannel !== ALL_CHANNELS_TAB && msg.channel !== view.selectedChannel) return false;
      if (view.direction !== 'any' && msg.direction !== view.direction) return false;
      if (cutoff && isoToMs(msg.external_created_at) < cutoff) return false;
      return true;
    });
  }

  async function refreshTimelineForActiveBucket() {
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
    if (!bucket) return;
    await loadTimelineForBucket(bucket);
    renderTimeline();
    renderRightPane();
  }

  async function applyConversationDeepLink() {
    const link = view.deepLink;
    if (!link || link.applied) return;
    if (hasOutboundDeepLink(link)) ensureOutboundContextCollectionsSync();
    if (link.channel && (SUPPORTED_CHANNELS.includes(link.channel) || link.channel === 'all')) {
      view.channel = link.channel === 'all' ? 'all' : link.channel;
      for (const btn of refs.channelFilterButtons) {
        const active = btn.dataset.channel === view.channel;
        btn.classList.toggle('is-active', active);
        btn.setAttribute('aria-pressed', String(active));
      }
    }
    if (link.account_key) {
      view.account = link.account_key;
      if (refs.accountFilter) refs.accountFilter.value = link.account_key;
    }
    if (link.message_key && messagesCollection) {
      try {
        const doc = await withTimeout(messagesCollection.findOne(link.message_key).exec(), 5000, null);
        const message = doc?.toJSON?.();
        if (message) {
          link.thread_key ||= message.thread_key || '';
          link.account_key ||= message.account_key || '';
          link.channel ||= message.channel || '';
          view.highlightedMessageKey = message.message_key || link.message_key;
        }
      } catch (error) {
        console.warn('[conversations] deep link message lookup failed:', error);
      }
    }
    const bucket = findBucketForDeepLink(link);
    if (bucket) {
      view.selectedBucketKey = bucket.key;
      view.selectedChannel = link.channel && bucket.channels.has(link.channel) ? link.channel : ALL_CHANNELS_TAB;
      view.timelineLimit = MESSAGE_PAGE_SIZE;
      link.resolved = true;
    }
    view.outboundContext = await waitForOutboundContextForLink(link);
    if (hasOutboundDeepLink(link) && !outboundContextSatisfiesDeepLink(view.outboundContext, link)) {
      scheduleOutboundContextRetry(link);
      return;
    }
    link.applied = true;
  }

  function hasOutboundDeepLink(link = {}) {
    return Boolean(link.outbound_message_id || link.engagement_id || link.campaign_id || link.thread_key || link.message_key);
  }

  function scheduleOutboundContextRetry(link = {}) {
    if (view.outboundContextRetryTimer) window.clearTimeout(view.outboundContextRetryTimer);
    let attempts = 0;
    const retry = async () => {
      attempts += 1;
      if (!String(location.hash || '').startsWith('#conversations')) return;
      ensureOutboundContextCollectionsSync();
      const context = await loadOutboundContextForLink(link).catch((error) => {
        console.warn('[conversations] outbound context retry failed:', error);
        return null;
      });
      if (outboundContextSatisfiesDeepLink(context, link)) {
        view.outboundContext = context;
        link.applied = true;
        renderRightPane();
        return;
      }
      if (attempts < 60) {
        view.outboundContextRetryTimer = window.setTimeout(retry, 500);
      }
    };
    view.outboundContextRetryTimer = window.setTimeout(retry, 500);
  }

  async function waitForOutboundContextForLink(link = {}, timeoutMs = 15000) {
    const deadline = Date.now() + timeoutMs;
    let context = null;
    while (Date.now() < deadline) {
      context = await loadOutboundContextForLink(link).catch((error) => {
        console.warn('[conversations] outbound context lookup skipped:', error);
        return context;
      });
      if (outboundContextSatisfiesDeepLink(context, link)) return context;
      await sleep(250);
    }
    return context;
  }

  function outboundContextSatisfiesDeepLink(context, link = {}) {
    if (!context) return false;
    if (link.outbound_message_id) return context.message?.id === link.outbound_message_id;
    if (link.engagement_id) return context.engagement?.id === link.engagement_id || context.message?.engagement_id === link.engagement_id;
    if (link.campaign_id) return context.campaign?.id === link.campaign_id || context.message?.campaign_id === link.campaign_id;
    return Boolean(context.message || context.engagement || context.campaign);
  }

  async function loadOutboundContextForLink(link = {}) {
    if (!outboundMessagesCollection && !outboundEngagementsCollection && !outboundCampaignsCollection) return null;
    let outboundMessage = null;
    if (link.outbound_message_id && outboundMessagesCollection) {
      outboundMessage = await findOneJson(outboundMessagesCollection, link.outbound_message_id);
    }
    if (!outboundMessage && outboundMessagesCollection && (link.message_key || link.thread_key || link.engagement_id)) {
      const docs = await withTimeout(outboundMessagesCollection.find().limit(250).exec(), 5000, []);
      outboundMessage = (docs || [])
        .map((doc) => doc.toJSON())
        .find((message) => (
          (link.message_key && (message.communication_message_key === link.message_key || message.payload?.communication_message_key === link.message_key))
          || (link.thread_key && (message.thread_key === link.thread_key || message.payload?.thread_key === link.thread_key))
          || (link.engagement_id && message.engagement_id === link.engagement_id)
        )) || null;
    }
    let engagement = null;
    const engagementId = link.engagement_id || outboundMessage?.engagement_id || '';
    if (engagementId && outboundEngagementsCollection) {
      engagement = await findOneJson(outboundEngagementsCollection, engagementId);
    }
    let campaign = null;
    const campaignId = link.campaign_id || outboundMessage?.campaign_id || engagement?.campaign_id || '';
    if (campaignId && outboundCampaignsCollection) {
      campaign = await findOneJson(outboundCampaignsCollection, campaignId);
    }
    let pipeline = null;
    let contact = null;
    if (engagement?.pipeline_id && outboundPipelineItemsCollection) {
      pipeline = await findOneJson(outboundPipelineItemsCollection, engagement.pipeline_id);
      contact = contactForOutboundEngagement(pipeline, engagement);
    }
    let approvals = [];
    if (outboundMessage?.id && outboundApprovalsCollection) {
      const docs = await withTimeout(outboundApprovalsCollection.find().limit(250).exec(), 5000, []);
      approvals = (docs || []).map((doc) => doc.toJSON()).filter((item) => item.message_id === outboundMessage.id);
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
    for (const thread of view.threads) {
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
    renderAccountFilterStatus(accounts.length);
  }

  function renderAccountFilterStatus(accountCount) {
    if (!refs.accountFilterStatus) return;
    const diagnostics = currentDataDiagnostics();
    const hasAccountFailure = diagnostics.problemCollections.includes('communication_accounts');
    refs.accountFilterStatus.dataset.state = hasAccountFailure ? 'error' : '';
    if (hasAccountFailure) {
      refs.accountFilterStatus.textContent = t('accountFilterSyncFailure', 'Accounts gerade nicht verfügbar');
    } else if (accountCount > 0) {
      refs.accountFilterStatus.textContent = `${accountCount} ${t('accountFilterCount', 'Accounts verfügbar')}`;
    } else {
      refs.accountFilterStatus.textContent = t('accountFilterEmpty', 'Keine Communication-Accounts synchronisiert');
    }
  }

  // ----- renderers -----

  function renderList() {
    const filtered = filterBuckets(view.buckets, view, t);
    refs.threadList.replaceChildren();
    updateChannelCounts(view.buckets);
    if (!filtered.length) {
      renderListEmptyState(filtered);
      refs.emptyList.hidden = false;
      if (view.selectedBucketKey) {
        view.selectedBucketKey = null;
        view.selectedChannel = ALL_CHANNELS_TAB;
        renderDetail();
      } else {
        renderRightPane();
      }
      return;
    }
    refs.emptyList.hidden = true;

    for (const bucket of filtered) {
      refs.threadList.appendChild(buildBucketItem(bucket));
    }

    if (view.selectedBucketKey && !filtered.some((b) => b.key === view.selectedBucketKey)) {
      view.selectedBucketKey = null;
      view.selectedChannel = ALL_CHANNELS_TAB;
      renderDetail();
      return;
    }
    if (!view.selectedBucketKey) {
      selectBucket(filtered[0].key);
    } else {
      markActiveBucket();
    }
  }

  function renderListEmptyState(filtered) {
    const state = conversationEmptyState({
      totalBuckets: view.buckets.length,
      filteredBuckets: filtered.length,
      hasActiveFilters: hasActiveListFilters(view),
      diagnostics: currentDataDiagnostics(),
      hasLocalCommunicationData: hasLocalCommunicationData(view),
      t,
    });
    const title = refs.emptyList.querySelector('[data-empty-list-title]');
    const body = refs.emptyList.querySelector('[data-empty-list-body]');
    if (title) title.textContent = state.title;
    if (body) body.textContent = state.body;
  }

  function buildBucketItem(bucket) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'conv-thread-item';
    btn.dataset.bucketKey = bucket.key;
    btn.setAttribute('role', 'option');
    if (view.selectedBucketKey === bucket.key) btn.classList.add('is-active');

    const avatar = document.createElement('span');
    avatar.className = 'conv-avatar';
    avatar.textContent = avatarGlyphFor(bucket.displayName);
    btn.appendChild(avatar);

    const newestThread = newestThreadOf(bucket);
    const body = document.createElement('div');
    body.className = 'conv-thread-body';
    body.innerHTML = `
      <div class="conv-thread-top">
        <span class="conv-thread-name"></span>
        <span class="conv-thread-time"></span>
      </div>
      <div class="conv-thread-preview">
        <span class="conv-thread-dir"></span>
        <span></span>
      </div>
      <div class="conv-thread-channels"></div>
    `;
    body.querySelector('.conv-thread-name').textContent = bucket.displayName;
    body.querySelector('.conv-thread-time').textContent = formatTimeShort(bucket.lastMessageAt);
    const dir = body.querySelector('.conv-thread-dir');
    dir.textContent = bucket.channels.size > 1 ? `${bucket.channels.size}×` : '·';
    dir.title = `${bucket.channels.size} ${t('threadCountSuffix', 'Threads')}`;
    body.querySelector('.conv-thread-preview span:last-child').textContent = newestThread?.subject || '';
    const channelsRow = body.querySelector('.conv-thread-channels');
    for (const channel of [...bucket.channels]) {
      const dot = document.createElement('span');
      dot.className = 'conv-channel-dot';
      dot.dataset.dot = channel;
      dot.title = labelForChannel(channel, t);
      channelsRow.appendChild(dot);
    }
    if (bucket.unreadCount > 0) {
      const badge = document.createElement('span');
      badge.className = 'conv-unread-badge';
      badge.textContent = String(bucket.unreadCount);
      body.querySelector('.conv-thread-top').appendChild(badge);
    }
    btn.appendChild(body);
    btn.addEventListener('click', () => selectBucket(bucket.key));
    return btn;
  }

  function selectBucket(bucketKey) {
    if (view.selectedBucketKey === bucketKey) {
      markActiveBucket();
      return;
    }
    view.selectedBucketKey = bucketKey;
    view.selectedChannel = ALL_CHANNELS_TAB;
    view.timelineLimit = MESSAGE_PAGE_SIZE;
    view.outboundContext = null;
    view.highlightedMessageKey = '';
    markActiveBucket();
    renderDetail();
  }

  function markActiveBucket() {
    for (const node of refs.threadList.querySelectorAll('.conv-thread-item')) {
      node.classList.toggle('is-active', node.dataset.bucketKey === view.selectedBucketKey);
    }
  }

  async function renderDetail() {
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
    if (!bucket) {
      refs.detailHeader.hidden = true;
      refs.channelTabs.hidden = true;
      refs.channelTabs.replaceChildren();
      refs.timeline.replaceChildren();
      refs.emptyDetail.hidden = false;
      renderRightEmptyState();
      refs.rightEmpty.hidden = false;
      refs.rightDetail.hidden = true;
      return;
    }
    refs.detailHeader.hidden = false;
    refs.emptyDetail.hidden = true;
    refs.detailAvatar.textContent = avatarGlyphFor(bucket.displayName);
    refs.detailName.textContent = bucket.displayName;
    refs.detailHandle.textContent = formatParticipantHandles(bucket.participants);

    renderChannelTabs(bucket);

    await loadTimelineForBucket(bucket);
    renderTimeline();
    renderRightPane();
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
      tab.className = 'conv-channel-tab';
      tab.setAttribute('role', 'tab');
      tab.dataset.channel = channel;
      if (channel === view.selectedChannel) {
        tab.classList.add('is-active');
        tab.setAttribute('aria-selected', 'true');
      }
      if (channel === ALL_CHANNELS_TAB) {
        tab.innerHTML = `<span></span><span class="conv-channel-tab-count"></span>`;
        tab.querySelector('span:first-child').textContent = t('channelAll', 'Alle Channels');
        tab.querySelector('.conv-channel-tab-count').textContent = String(bucket.messageCount || sumValues(messageCountByChannel));
      } else {
        tab.innerHTML = `
          <span class="conv-channel-dot" data-dot="${channel}"></span>
          <span></span>
          <span class="conv-channel-tab-count"></span>
        `;
        tab.querySelectorAll('span')[1].textContent = labelForChannel(channel, t);
        tab.querySelector('.conv-channel-tab-count').textContent = String(messageCountByChannel.get(channel) || 0);
      }
      tab.addEventListener('click', () => {
        view.selectedChannel = channel;
        view.timelineLimit = MESSAGE_PAGE_SIZE;
        for (const other of refs.channelTabs.querySelectorAll('.conv-channel-tab')) {
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
    if (!view.timelineMessages.length) {
      const empty = document.createElement('div');
      empty.className = 'conv-empty';
      empty.innerHTML = `<span></span>`;
      empty.querySelector('span').textContent = t('emptyDetailBody', 'Wähle links eine Konversation.');
      refs.timeline.appendChild(empty);
      return;
    }
    if (view.timelineTotal > view.timelineMessages.length) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'conv-load-older';
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
    } else {
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
        toggle.className = 'conv-message-html-toggle';
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
      pill.className = 'conv-attachment';
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
      status.className = 'conv-message-status';
      status.innerHTML = `<span class="conv-message-status-icon"></span><span></span>`;
      status.querySelector('.conv-message-status-icon').textContent = iconForStatus(msg.status);
      status.querySelectorAll('span')[1].textContent = labelForStatus(msg.status, t);
      el.appendChild(status);
    }

    const taskBadge = buildTaskBadge(msg);
    if (taskBadge) el.appendChild(taskBadge);

    el.appendChild(buildDetailsBlock(msg));

    el.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      event.stopPropagation();
      openMessageContextMenu(event, msg);
    });

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

    if (taskId) {
      wrap.appendChild(buildTaskChip({
        kind: 'task',
        label: t('taskBadgeLabel', 'Task'),
        id: taskId,
        routeStatus: msg.route_status || '',
        onActivate: () => navigateToFlowview({ taskId, sourceModule: 'conversations' }),
      }));
    }
    if (workId) {
      wrap.appendChild(buildTaskChip({
        kind: 'work',
        label: t('workBadgeLabel', 'Work'),
        id: workId,
        routeStatus: msg.route_status || '',
        onActivate: () => navigateToFlowview({ workId, taskId: taskId || msg.message_key, sourceModule: 'conversations' }),
      }));
    }
    return wrap;
  }

  function buildTaskChip({ kind, label, id, routeStatus, onActivate }) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'conv-message-task-chip';
    btn.dataset.kind = kind;
    if (routeStatus) btn.dataset.routeStatus = routeStatus;
    btn.title = `${label}: ${id}${routeStatus ? ` · ${labelForRouteStatus(routeStatus, t)}` : ''}`;
    btn.innerHTML = `
      <span class="conv-message-task-chip-kind"></span>
      <span class="conv-message-task-chip-id"></span>
      <span class="conv-message-task-chip-status"></span>
      <span class="conv-message-task-chip-jump">↗</span>
    `;
    btn.querySelector('.conv-message-task-chip-kind').textContent = label;
    btn.querySelector('.conv-message-task-chip-id').textContent = shortenId(id);
    const statusEl = btn.querySelector('.conv-message-task-chip-status');
    if (routeStatus) {
      statusEl.textContent = labelForRouteStatus(routeStatus, t);
      statusEl.dataset.routeStatus = routeStatus;
    } else {
      statusEl.hidden = true;
    }
    btn.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      onActivate();
    });
    return btn;
  }

  function navigateToFlowview({ taskId = '', workId = '', sourceModule = 'conversations' } = {}) {
    if (!taskId && !workId) return;
    const params = new URLSearchParams();
    if (taskId) params.set('task_id', taskId);
    if (workId) params.set('work_id', workId);
    if (sourceModule) params.set('source', sourceModule);
    const next = `#ctox?${params.toString()}`;
    if (location.hash === next) {
      window.dispatchEvent(new HashChangeEvent('hashchange'));
    } else {
      location.hash = next;
    }
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
    if (ctx.openDesktopApp) {
      ctx.openDesktopApp('file-viewer', {
        args: {
          messageKey: msg.message_key,
          channel: msg.channel,
          ref,
          metadata: msg.metadata_json || {},
        },
      });
    }
  }

  function openMessageContextMenu(event, msg) {
    if (!ctx.contextMenu) return;
    const isInbound = msg.direction === 'inbound';
    const taskId = isInbound ? msg.message_key : '';
    const workId = msg.ticket_self_work_id || msg.work_id || '';

    const items = [];
    if (taskId || workId) {
      items.push({
        label: t('contextOpenFlowview', 'Im CTOX-Flow öffnen'),
        icon: '↗',
        action: () => navigateToFlowview({ taskId, workId, sourceModule: 'conversations' }),
      });
      items.push({ type: 'separator' });
    }
    items.push({
      label: 'Chat to CTOX',
      icon: '💬',
      action: () => {
        spawnMessageCtoxForm(event.clientX, event.clientY, msg);
      }
    });
    items.push({ type: 'separator' });
    items.push({
      label: t('contextCopyId', 'Message-Key kopieren'),
      icon: '⧉',
      action: () => copyToClipboard(msg.message_key),
    });
    if (workId) {
      items.push({
        label: t('contextCopyWorkId', 'Work-ID kopieren'),
        icon: '⧉',
        action: () => copyToClipboard(workId),
      });
    }
    if (msg.remote_id) {
      items.push({
        label: t('contextCopyRemoteId', 'Remote-ID kopieren'),
        icon: '⧉',
        action: () => copyToClipboard(msg.remote_id),
      });
    }
    if (msg.body_text || msg.preview) {
      items.push({
        label: t('contextCopyBody', 'Inhalt kopieren'),
        icon: '⧉',
        action: () => copyToClipboard(msg.body_text || msg.preview),
      });
    }
    if (Number(msg.has_attachments) > 0 && msg.raw_payload_ref) {
      items.push({ type: 'separator' });
      items.push({
        label: t('contextOpenAttachment', 'Anhang öffnen'),
        icon: '📎',
        action: () => openAttachment(msg),
      });
    }
    ctx.contextMenu.show(event, items);
  }

  function spawnMessageCtoxForm(x, y, msg) {
    const menu = document.createElement('div');
    menu.className = 'ctox-context-menu';
    menu.style.position = 'fixed';
    menu.hidden = true;
    document.body.append(menu);

    const escapeHtml = (val) => String(val || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');

    const label = msg.subject || msg.sender_address || 'Nachricht';

    menu.innerHTML = `
      <form class="conversations-context-chat" data-conversations-context-form>
        <header>
          <div>
            <strong>Chat to CTOX</strong>
            <span>Nachricht · ${escapeHtml(msg.channel || 'conversations')}</span>
          </div>
          <button type="button" data-context-close aria-label="Schließen">×</button>
        </header>
        <div class="conversations-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
        <textarea data-context-message placeholder="Was soll CTOX hier tun oder prüfen?"></textarea>
        <footer>
          <span data-context-status></span>
          <button type="submit">Senden</button>
        </footer>
      </form>
    `;

    menu.hidden = false;
    const rect = menu.getBoundingClientRect();
    const left = Math.min(x, window.innerWidth - rect.width - 8);
    const top = Math.min(y, window.innerHeight - rect.height - 8);
    menu.style.left = `${Math.max(8, left)}px`;
    menu.style.top = `${Math.max(8, top)}px`;

    const form = menu.querySelector('[data-conversations-context-form]');
    const textarea = menu.querySelector('[data-context-message]');
    const status = menu.querySelector('[data-context-status]');

    menu.querySelector('[data-context-close]')?.addEventListener('click', () => {
      menu.remove();
    });

    const handleOutside = (e) => {
      if (!menu.contains(e.target)) {
        menu.remove();
        document.removeEventListener('click', handleOutside, true);
      }
    };
    document.addEventListener('click', handleOutside, true);

    form?.addEventListener('submit', (event) => {
      event.preventDefault();
      const instruction = String(textarea?.value || '').trim();
      if (!instruction) {
        textarea?.focus();
        return;
      }
      const mode = new FormData(form).get('contextMode') || 'data';
      status.textContent = 'Gesendet.';

      const title = `${mode === 'app' ? 'Conversations App modifizieren' : 'Nachricht bearbeiten'} · ${msg.channel || 'conversations'}`;
      const context = {
        module: 'conversations',
        column: msg.channel || 'conversations',
        record_type: 'message',
        record_id: msg.message_key || '',
        label,
        text: (msg.body_text || msg.preview || '').slice(0, 240)
      };

      window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
        detail: {
          text: instruction,
          module: 'conversations',
          source_title: 'Conversations',
          command_type: mode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
          record_id: msg.message_key || 'conversations',
          title,
          instruction,
          payload: {
            title,
            instruction,
            prompt: instruction,
            user_message: instruction,
            mode,
            target: mode === 'app' ? 'app' : 'data',
            selected_message: msg,
            context,
            thread_key: 'business-os/conversations',
          },
          client_context: {
            action: 'context-chat',
            mode,
            column: msg.channel,
            message_key: msg.message_key,
          },
        },
      }));

      setTimeout(() => {
        menu.remove();
        document.removeEventListener('click', handleOutside, true);
      }, 650);
    });

    requestAnimationFrame(() => textarea?.focus());
  }

  function copyToClipboard(value) {
    if (!value) return;
    const text = String(value);
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(text).then(() => {
        ctx.notifications?.show?.({
          type: 'info',
          title: t('copiedToast', 'In Zwischenablage kopiert'),
          message: text.length > 60 ? `${text.slice(0, 57)}…` : text,
        });
      }).catch((error) => console.error('[conversations] clipboard write failed:', error));
    }
  }

  function renderRightPane() {
    const bucket = view.buckets.find((b) => b.key === view.selectedBucketKey);
    if (!bucket) {
      renderRightEmptyState();
      refs.rightEmpty.hidden = false;
      refs.rightDetail.hidden = true;
      return;
    }
    refs.rightEmpty.hidden = true;
    refs.rightDetail.hidden = false;
    refs.contactAvatar.textContent = avatarGlyphFor(bucket.displayName);
    refs.contactName.textContent = bucket.displayName;
    refs.contactHandle.textContent = formatParticipantHandles(bucket.participants);

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

  function renderRightEmptyState() {
    const state = conversationEmptyState({
      totalBuckets: view.buckets.length,
      filteredBuckets: filterBuckets(view.buckets, view, t).length,
      hasActiveFilters: hasActiveListFilters(view),
      diagnostics: currentDataDiagnostics(),
      hasLocalCommunicationData: hasLocalCommunicationData(view),
      t,
    });
    if (refs.rightEmptyTitle) refs.rightEmptyTitle.textContent = state.kind === 'initial-empty' ? '' : state.title;
    if (refs.rightEmptyBody) refs.rightEmptyBody.textContent = state.kind === 'initial-empty'
      ? t('rightEmptyBody', 'Kontaktdaten und verknüpfte Datensätze erscheinen hier.')
      : state.body;
  }

  function renderOutboundCard(bucket) {
    if (!refs.outboundCard || !refs.outboundBody) return;
    const loadedContext = view.outboundContext || outboundContextFromBucket(bucket);
    const fallbackContext = outboundContextFromDeepLinkFallback(bucket);
    const context = mergeOutboundContexts(loadedContext, fallbackContext);
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
      statusNote: view.outboundStatus,
    });
    refs.outboundBody.appendChild(summary);

    const rows = outboundFactRows({ message, engagement, latestApproval, leadName, t });
    if (rows.length) {
      const dl = document.createElement('dl');
      dl.className = 'conv-outbound-facts';
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
    openBtn.className = 'conv-action-button';
    openBtn.dataset.action = 'conv-outbound-open';
    openBtn.textContent = t('outboundOpenCampaign', 'In Outbound öffnen');
    openBtn.addEventListener('click', () => openOutboundContext(context));
    actions.appendChild(openBtn);

    if (canApproveOutboundMessage(message)) {
      const approveBtn = document.createElement('button');
      approveBtn.type = 'button';
      approveBtn.className = 'conv-action-button conv-action-button--primary';
      approveBtn.dataset.action = 'conv-outbound-approve';
      approveBtn.dataset.messageId = message.id;
      approveBtn.textContent = t('outboundApprove', 'Freigeben');
      approveBtn.addEventListener('click', () => approveOutboundMessageFromConversations(message.id));
      actions.appendChild(approveBtn);

      const rejectBtn = document.createElement('button');
      rejectBtn.type = 'button';
      rejectBtn.className = 'conv-action-button';
      rejectBtn.dataset.action = 'conv-outbound-reject';
      rejectBtn.dataset.messageId = message.id;
      rejectBtn.textContent = t('outboundReject', 'Ablehnen');
      rejectBtn.addEventListener('click', () => rejectOutboundMessageFromConversations(message.id));
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
        <select></select>
      </label>
      <button type="button" class="conv-action-button conv-action-button--primary"></button>
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
    wrap.querySelector('button').textContent = t('outboundReplyClassify', 'Klassifizieren');
    wrap.querySelector('button').dataset.action = 'conv-outbound-reply-classify';
    wrap.querySelector('button').dataset.engagementId = context.engagement.id;
    wrap.querySelector('small').textContent = `${t('outboundReplyLabel', 'Antwort')}: ${shortenId(inboundReply.message_key || inboundReply.remote_id || '')}`;
    wrap.querySelector('button').addEventListener('click', async () => {
      await classifyOutboundReply({
        engagementId: context.engagement.id,
        replyMessageKey: inboundReply.message_key || '',
        classification: select.value,
      });
    });
    return wrap;
  }

  function outboundContextFromBucket(bucket) {
    if (view.outboundContext || !bucket) return view.outboundContext;
    const threadContext = bucket.threads
      .map((thread) => outboundLinkFromMetadata(thread.metadata_json))
      .find(Boolean);
    const messageContext = view.timelineMessages
      .map((message) => outboundLinkFromMetadata(message.metadata_json))
      .find(Boolean);
    const link = threadContext || messageContext;
    if (!link) return null;
    loadOutboundContextForLink(link).then((context) => {
      view.outboundContext = context;
      renderRightPane();
    }).catch((error) => console.warn('[conversations] outbound context lookup failed:', error));
    return null;
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

  function outboundStatusLine({ engagement = {}, message = {}, latestApproval = null, statusNote = '' } = {}) {
    const parts = [];
    if (statusNote) parts.push(statusNote);
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
        return t('outboundApprovalAwaiting', 'Wartet auf Freigabe');
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
    const next = params.toString() ? `#outbound?${params.toString()}` : '#outbound';
    if (location.hash === next) {
      window.dispatchEvent(new HashChangeEvent('hashchange'));
    } else {
      location.hash = next;
    }
  }

  async function approveOutboundMessageFromConversations(messageId) {
    view.outboundStatus = t('outboundApprovalRequested', 'Freigabe angefordert');
    renderRightPane();
    const outcome = await materializeOutboundDecisionLocally(messageId, 'approved');
    dispatchOutboundCommand('outbound.message.approve', messageId, { message_id: messageId }, { waitForOutcome: false, timeoutMs: 1500 })
      .catch((error) => console.warn('[conversations] outbound approve command dispatch failed:', error));
    view.outboundStatus = t('outboundApprove', 'Freigeben');
    await applyOutboundDecisionToCommunicationMessage(outcome, messageId, 'approved');
    view.outboundContext = await waitForOutboundContextForLink({ ...view.deepLink, outbound_message_id: messageId });
    if (!view.outboundContext || view.outboundContext.message?.approval_status === 'awaiting_approval') {
      scheduleOutboundContextRetry({ ...view.deepLink, outbound_message_id: messageId });
    }
    mergeOutboundCommandOutcomeIntoContext(outcome);
    renderRightPane();
  }

  async function rejectOutboundMessageFromConversations(messageId) {
    view.outboundStatus = t('outboundRejectionRequested', 'Ablehnung angefordert');
    renderRightPane();
    const outcome = await materializeOutboundDecisionLocally(messageId, 'rejected', 'Rejected from Conversations');
    dispatchOutboundCommand('outbound.message.reject', messageId, {
      message_id: messageId,
      comment: 'Rejected from Conversations',
    }, { waitForOutcome: false, timeoutMs: 1500 })
      .catch((error) => console.warn('[conversations] outbound reject command dispatch failed:', error));
    view.outboundStatus = t('outboundReject', 'Ablehnen');
    await applyOutboundDecisionToCommunicationMessage(outcome, messageId, 'rejected');
    view.outboundContext = await waitForOutboundContextForLink({ ...view.deepLink, outbound_message_id: messageId });
    if (!view.outboundContext || view.outboundContext.message?.approval_status === 'awaiting_approval') {
      scheduleOutboundContextRetry({ ...view.deepLink, outbound_message_id: messageId });
    }
    mergeOutboundCommandOutcomeIntoContext(outcome);
    renderRightPane();
  }

  async function classifyOutboundReply({ engagementId, replyMessageKey, classification }) {
    await dispatchOutboundCommand('outbound.reply.classify', engagementId, {
      engagement_id: engagementId,
      classification,
      reply_message_id: replyMessageKey,
    });
    if (view.outboundContext?.engagement?.id === engagementId) {
      view.outboundContext.engagement.status = 'reply_received';
      view.outboundContext.engagement.payload = {
        ...(view.outboundContext.engagement.payload || {}),
        reply_classification: classification,
        reply_message_id: replyMessageKey,
      };
    }
    renderRightPane();
  }

  function maybeAutoMatchOutboundReply(context = {}, inboundReply = {}) {
    const engagementId = context.engagement?.id || context.message?.engagement_id || '';
    const replyMessageKey = inboundReply.message_key || '';
    if (!engagementId || !replyMessageKey) return;
    if (context.engagement?.payload?.reply_message_id === replyMessageKey) return;
    const dedupeKey = `${engagementId}:${replyMessageKey}`;
    if (view.outboundReplyMatchKeys.has(dedupeKey)) return;
    view.outboundReplyMatchKeys.add(dedupeKey);
    dispatchOutboundCommand('outbound.reply.match', engagementId, {
      engagement_id: engagementId,
      outbound_message_id: context.message?.id || '',
      reply_message_id: replyMessageKey,
      classification: context.engagement?.payload?.reply_classification || 'unclear',
    }).then(() => {
      if (view.outboundContext?.engagement?.id === engagementId) {
        view.outboundContext.engagement.status = 'reply_received';
        view.outboundContext.engagement.payload = {
          ...(view.outboundContext.engagement.payload || {}),
          reply_message_id: replyMessageKey,
          reply_classification: context.engagement?.payload?.reply_classification || 'unclear',
        };
      }
      view.outboundStatus = t('outboundReplyMatched', 'Antwort automatisch zugeordnet');
      renderRightPane();
    }).catch((error) => {
      view.outboundReplyMatchKeys.delete(dedupeKey);
      console.warn('[conversations] outbound reply auto-match failed:', error);
    });
  }

  async function dispatchOutboundCommand(type, recordId, payload, options = {}) {
    const commandId = `cmd_${type.replaceAll('.', '_')}_${crypto.randomUUID()}`;
    const command = {
      id: commandId,
      command_id: commandId,
      module: 'outbound',
      type,
      command_type: type,
      record_id: recordId || '',
      payload,
      inbound_channel: 'business_os.conversations',
      client_context: {
        source_module: 'conversations',
        deep_link: view.deepLink || {},
      },
      created_at_ms: Date.now(),
      updated_at_ms: Date.now(),
    };
    const dispatched = ctx.commandBus?.dispatch
      ? await withTimeout(ctx.commandBus.dispatch(command), options.timeoutMs || 5000, null).catch((error) => {
          console.warn('[conversations] outbound command bus dispatch failed, using RxDB fallback:', error);
          return null;
        })
      : null;
    if (!dispatched) {
      await insertOutboundCommandFallback(command);
    }
    if (options.waitForOutcome === false) {
      return dispatched || { id: commandId, command_id: commandId, status: 'pending_sync', pending: true };
    }
    const outcome = await waitForOutboundCommandOutcome(commandId);
    ctx.notifications?.show?.({
      type: 'info',
      title: 'Outbound',
      message: type,
    });
    return outcome;
  }

  function scheduleOutboundCommandDispatch(type, recordId, payload, attempt = 0) {
    if (!isBusinessCommandsReadyForDispatch()) {
      if (attempt < 60) {
        window.setTimeout(() => scheduleOutboundCommandDispatch(type, recordId, payload, attempt + 1), 1000);
      }
      return;
    }
    dispatchOutboundCommand(type, recordId, payload, { waitForOutcome: false, timeoutMs: 1500 })
      .catch((error) => {
        if (attempt < 60) {
          window.setTimeout(() => scheduleOutboundCommandDispatch(type, recordId, payload, attempt + 1), 1000);
          return;
        }
        console.warn('[conversations] outbound command dispatch deferred failed:', error);
      });
  }

  function isBusinessCommandsReadyForDispatch() {
    const diagnostics = view.syncDiagnostics?.collections?.business_commands
      || ctx.sync?.diagnostics?.collections?.business_commands
      || window.ctoxBusinessOsSyncDiagnostics?.collections?.business_commands
      || null;
    if (!businessCommandsCollection || !diagnostics) return false;
    const status = diagnostics.connectionStatus || diagnostics.status || '';
    const activePeerCount = Number(diagnostics.frameTransport?.activePeerCount || 0);
    return ['connected', 'reused', 'running'].includes(status)
      && activePeerCount > 0
      && Boolean(diagnostics.initialReplicationAt || diagnostics.initialReplicationState === 'complete');
  }

  async function materializeOutboundDecisionLocally(messageId, decision, comment = '') {
    if (!outboundMessagesCollection) throw new Error('outbound_messages collection is required for Conversations outbound decisions');
    const doc = await outboundMessagesCollection.findOne(messageId).exec();
    if (!doc) throw new Error(`Outbound message missing: ${messageId}`);
    const current = doc.toJSON?.() || {};
    const approved = decision === 'approved';
    const now = Date.now();
    const revisionId = current.revision_id || `rev_${crypto.randomUUID().replaceAll('-', '')}`;
    const messagePatch = {
      draft_status: approved ? 'approved' : 'changes_requested',
      approval_status: approved ? 'approved' : 'rejected',
      send_status: approved ? 'approved_not_sent' : 'cancelled',
      revision_id: revisionId,
      updated_at_ms: now,
      _meta: {
        ...(current._meta || {}),
        lwt: now,
      },
    };
    const message = { ...current, ...messagePatch };
    await retryRxdbWrite(async () => {
      await outboundMessagesCollection.upsert(message);
    }, 'outbound message decision');
    const approval = {
      id: `approval_${messageId}_${revisionId}`.slice(0, 180),
      message_id: messageId,
      engagement_id: message.engagement_id || view.outboundContext?.engagement?.id || '',
      revision_id: revisionId,
      actor_user_id: ctx.session?.user?.id || 'business-os.conversations',
      decision: approved ? 'approved' : 'rejected',
      comment,
      payload: { source_module: 'conversations' },
      created_at_ms: now,
      updated_at_ms: now,
    };
    if (outboundApprovalsCollection) {
      await retryRxdbWrite(() => outboundApprovalsCollection.upsert(approval), 'outbound approval audit');
    }
    if (view.outboundContext?.message?.id === messageId) {
      view.outboundContext.message = { ...view.outboundContext.message, ...message };
      const approvals = Array.isArray(view.outboundContext.approvals)
        ? view.outboundContext.approvals.filter((item) => item.id !== approval.id)
        : [];
      approvals.unshift(approval);
      view.outboundContext.approvals = approvals;
    }
    return { ok: true, message, approval };
  }

  async function retryRxdbWrite(operation, label, timeoutMs = 60000) {
    const deadline = Date.now() + timeoutMs;
    let lastError = null;
    const timeoutSentinel = Symbol(`${label}.timeout`);
    while (Date.now() < deadline) {
      try {
        const result = await withTimeout(operation(), 10000, timeoutSentinel);
        if (result === timeoutSentinel) throw new Error(`${label} write timed out`);
        return result;
      } catch (error) {
        lastError = error;
      }
      await sleep(750);
    }
    throw lastError || new Error(`${label} timed out`);
  }

  async function insertOutboundCommandFallback(command) {
    if (!businessCommandsCollection) {
      throw new Error('business_commands collection is required for Conversations outbound command fallback');
    }
    const existing = await findOneJson(businessCommandsCollection, command.id).catch(() => null);
    if (existing) return;
    await businessCommandsCollection.insert({
      ...command,
      status: 'pending_sync',
    });
  }

  function withTimeout(promise, timeoutMs, fallback) {
    return Promise.race([
      promise,
      new Promise((resolve) => setTimeout(() => resolve(fallback), timeoutMs)),
    ]);
  }

  async function waitForOutboundCommandOutcome(commandId, timeoutMs = 60000) {
    if (!businessCommandsCollection) return null;
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const command = await findOneJson(businessCommandsCollection, commandId);
      const status = String(command?.status || '').toLowerCase();
      if (status === 'completed') return command;
      if (status === 'failed') {
        throw new Error(command?.error || command?.result?.error || `Outbound command failed: ${commandId}`);
      }
      await sleep(250);
    }
    return {
      id: commandId,
      command_id: commandId,
      status: 'pending_sync',
      pending: true,
    };
  }

  function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  function mergeOutboundCommandOutcomeIntoContext(outcome) {
    const result = outcome?.result || outcome;
    if (!result || !view.outboundContext) return;
    if (result.message?.id) {
      view.outboundContext.message = {
        ...(view.outboundContext.message || {}),
        ...result.message,
      };
    }
    if (result.engagement?.id) {
      view.outboundContext.engagement = {
        ...(view.outboundContext.engagement || {}),
        ...result.engagement,
      };
    }
    if (result.approval?.id) {
      const approvals = Array.isArray(view.outboundContext.approvals)
        ? view.outboundContext.approvals.filter((approval) => approval.id !== result.approval.id)
        : [];
      approvals.unshift(result.approval);
      view.outboundContext.approvals = approvals;
    }
  }

  async function applyOutboundDecisionToCommunicationMessage(outcome, fallbackMessageId, decision) {
    const result = outcome?.result || outcome || {};
    const message = result.message || view.outboundContext?.message || {};
    const communicationMessageKey = cleanParam(
      message.communication_message_key
        || message.payload?.communication_message_key
        || view.deepLink?.message_key
        || ''
    );
    if (!communicationMessageKey || !messagesCollection) return;

    const approved = decision === 'approved';
    const approvalStatus = approved ? 'approved' : 'rejected';
    const sendStatus = approved ? 'approved_not_sent' : 'cancelled';
    const nextAction = approved ? 'In Outbound versenden' : 'Nachricht ueberarbeiten';
    const nowIso = new Date().toISOString();

    try {
      const doc = await messagesCollection.findOne(communicationMessageKey).exec();
      if (!doc) return;
      const current = doc.toJSON?.() || {};
      const metadata = {
        ...(current.metadata_json && typeof current.metadata_json === 'object' ? current.metadata_json : {}),
        approval_status: approvalStatus,
        send_status: sendStatus,
        outbound_message_id: cleanParam(message.id || fallbackMessageId),
        outbound_engagement_id: cleanParam(message.engagement_id || view.outboundContext?.engagement?.id || ''),
        outbound_campaign_id: cleanParam(message.campaign_id || view.outboundContext?.campaign?.id || ''),
        communication_message_key: communicationMessageKey,
      };
      await doc.incrementalPatch({
        status: current.status || 'draft',
        folder_hint: current.folder_hint || 'drafts',
        metadata_json: metadata,
        updated_at: nowIso,
      });

      const localMessage = view.timelineMessages.find((item) => item.message_key === communicationMessageKey);
      if (localMessage) {
        localMessage.metadata_json = metadata;
        localMessage.status = localMessage.status || 'draft';
        localMessage.folder_hint = localMessage.folder_hint || 'drafts';
        localMessage.updated_at = nowIso;
      }
      if (view.outboundContext?.message) {
        view.outboundContext.message = {
          ...view.outboundContext.message,
          approval_status: approvalStatus,
          send_status: sendStatus,
          communication_message_key: communicationMessageKey,
        };
      }
      view.outboundStatus = approved
        ? t('outboundApprovalApplied', 'Freigegeben')
        : t('outboundRejectionApplied', 'Abgelehnt');
      if (view.outboundContext?.message) {
        view.outboundContext.message.payload = {
          ...(view.outboundContext.message.payload || {}),
          next_action_label: nextAction,
        };
      }
    } catch (error) {
      console.warn('[conversations] communication message decision projection failed:', error);
    }
  }

  function renderAccountsCard(bucket) {
    if (!refs.accountsBody) return;
    refs.accountsBody.replaceChildren();
    const accounts = [...bucket.accountKeys]
      .map((key) => view.accountsById.get(key))
      .filter(Boolean);
    if (!accounts.length) {
      const empty = document.createElement('span');
      empty.style.color = 'var(--muted)';
      empty.style.fontSize = '12px';
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
    const health = healthStateFor(account);
    row.innerHTML = `
      <span class="conv-account-health" data-state="${health.state}" title="${health.tooltip}"></span>
      <div class="conv-account-meta">
        <strong></strong>
        <div class="conv-account-meta-line"></div>
        <div class="conv-account-meta-line"></div>
      </div>
    `;
    row.querySelector('strong').textContent = `${labelForChannel(account.channel, t)} · ${account.address || account.account_key}`;
    const lines = row.querySelectorAll('.conv-account-meta-line');
    const provider = account.provider ? `${account.provider}` : '';
    lines[0].textContent = provider;
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

  function updateChannelCounts(buckets) {
    const counts = new Map();
    counts.set('all', buckets.length);
    for (const channel of SUPPORTED_CHANNELS) counts.set(channel, 0);
    for (const bucket of buckets) {
      for (const channel of bucket.channels) {
        counts.set(channel, (counts.get(channel) || 0) + 1);
      }
    }
    for (const btn of refs.channelFilterButtons) {
      const channel = btn.dataset.channel;
      const countEl = btn.querySelector('[data-channel-count]');
      const count = counts.get(channel) || 0;
      countEl.textContent = count > 0 ? String(count) : '';
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

  function currentDataDiagnostics() {
    return buildConversationDataDiagnostics({
      requiredCollections: COMMUNICATION_DIAGNOSTIC_COLLECTIONS,
      missingCollections: view.missingCollections,
      collectionErrors: view.collectionErrors,
      syncDiagnostics: view.syncDiagnostics || ctx.sync?.diagnostics || window.ctoxBusinessOsSyncDiagnostics || null,
    });
  }
}

// ----- helpers (pure / DOM-agnostic) -----

function collectRefs(root) {
  return {
    threadList: root.querySelector('[data-conv-thread-list]'),
    search: root.querySelector('[data-conv-search]'),
    channelFilterButtons: Array.from(root.querySelectorAll('[data-conv-channel-filters] [data-channel]')),
    accountFilter: root.querySelector('[data-conv-account-filter]'),
    accountFilterStatus: root.querySelector('[data-conv-account-filter-status]'),
    directionFilter: root.querySelector('[data-conv-direction-filter]'),
    dateFilter: root.querySelector('[data-conv-date-filter]'),
    emptyList: root.querySelector('[data-conv-empty-list]'),
    emptyDetail: root.querySelector('[data-conv-empty-detail]'),
    detailHeader: root.querySelector('[data-conv-detail-header]'),
    detailAvatar: root.querySelector('[data-conv-detail-avatar]'),
    detailName: root.querySelector('[data-conv-detail-name]'),
    detailHandle: root.querySelector('[data-conv-detail-handle]'),
    channelTabs: root.querySelector('[data-conv-channel-tabs]'),
    timeline: root.querySelector('[data-conv-timeline]'),
    rightEmpty: root.querySelector('[data-conv-right-empty]'),
    rightEmptyTitle: root.querySelector('[data-right-empty-title]'),
    rightEmptyBody: root.querySelector('[data-right-empty-body]'),
    rightDetail: root.querySelector('[data-conv-right-detail]'),
    contactAvatar: root.querySelector('[data-conv-contact-avatar]'),
    contactName: root.querySelector('[data-conv-contact-name]'),
    contactHandle: root.querySelector('[data-conv-contact-handle]'),
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
    '[data-empty-list-title]': 'emptyListTitle',
    '[data-empty-list-body]': 'emptyListBody',
    '[data-empty-detail-title]': 'emptyDetailTitle',
    '[data-empty-detail-body]': 'emptyDetailBody',
    '[data-right-empty-body]': 'rightEmptyBody',
    '[data-conv-card-label-contact]': 'cardLabelContact',
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
  const search = root.querySelector('[data-conv-search]');
  if (search) search.placeholder = t('searchPlaceholder', 'Suche');
  const labelByChannel = {
    all: t('channelAll', 'Alle Channels'),
    whatsapp: t('channelWhatsapp', 'WhatsApp'),
    email: t('channelEmail', 'E-Mail'),
    jami: t('channelJami', 'Jami'),
    teams: t('channelTeams', 'MS Teams'),
    meeting: t('channelMeeting', 'Meeting'),
  };
  for (const btn of root.querySelectorAll('[data-conv-channel-filters] [data-channel]')) {
    const channel = btn.dataset.channel;
    const labelEl = btn.querySelector('[data-channel-label]');
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

function wireChannelFilters(buttons, view, onChange) {
  for (const btn of buttons) {
    btn.setAttribute('aria-pressed', String(btn.classList.contains('is-active')));
    btn.addEventListener('click', () => {
      const channel = btn.dataset.channel || 'all';
      if (view.channel === channel) return;
      view.channel = channel;
      for (const other of buttons) {
        const active = other === btn;
        other.classList.toggle('is-active', active);
        other.setAttribute('aria-pressed', String(active));
      }
      onChange();
    });
  }
}

function filterBuckets(buckets, view, t) {
  const cutoff = dateCutoff(view.dateRange);
  return buckets.filter((bucket) => {
    if (view.channel !== 'all' && !bucket.channels.has(view.channel)) return false;
    if (view.account && !bucket.accountKeys.has(view.account)) return false;
    if (cutoff && isoToMs(bucket.lastMessageAt) < cutoff) return false;
    if (view.search) {
      const hay = [
        bucket.displayName,
        ...bucket.participants,
        ...[...bucket.subjects],
        ...bucket.threads.map((th) => th.account_key),
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

function hasLocalCommunicationData(view = {}) {
  return Boolean(
    view.accountsById?.size > 0
    || Number(view.messageProbeCount) > 0
    || Number(view.threads?.length) > 0,
  );
}

function conversationEmptyState({
  totalBuckets = 0,
  filteredBuckets = 0,
  hasActiveFilters = false,
  diagnostics = {},
  hasLocalCommunicationData: localData = false,
  t = (key, fallback) => fallback || key,
} = {}) {
  if (diagnostics.hasFailure) {
    return {
      kind: 'sync-failure',
      title: t('syncFailureTitle', 'Kommunikation ist gerade nicht verfügbar'),
      body: t('syncFailureBody', 'Konversationen erscheinen automatisch, sobald Kommunikationsdaten geladen sind.'),
    };
  }
  if (totalBuckets > 0 && filteredBuckets === 0 && hasActiveFilters) {
    return {
      kind: 'no-results',
      title: t('noResultsTitle', 'Keine Treffer'),
      body: t('noResultsBody', 'Keine Konversation passt zu den aktiven Filtern.'),
    };
  }
  if (totalBuckets === 0 && localData) {
    return {
      kind: 'projection-missing',
      title: t('projectionMissingTitle', 'Konversationen werden vorbereitet'),
      body: t('projectionMissingBody', 'Accounts und Nachrichten werden gerade für die Kontaktansicht vorbereitet.'),
    };
  }
  if (diagnostics.isStarting) {
    return {
      kind: 'sync-starting',
      title: t('syncStartingTitle', 'Kommunikation wird synchronisiert'),
      body: t('syncStartingBody', 'Accounts und Nachrichten sind noch nicht vollständig in der lokalen App angekommen.'),
    };
  }
  return {
    kind: 'initial-empty',
    title: t('emptyListTitle', 'Keine Konversationen'),
    body: t('emptyListBody', 'Hier erscheinen Kommunikationen, die CTOX über WhatsApp, E-Mail, Jami, MS Teams oder Meeting führt.'),
  };
}

function buildConversationDataDiagnostics({
  requiredCollections = COMMUNICATION_DIAGNOSTIC_COLLECTIONS,
  missingCollections = new Set(),
  collectionErrors = new Map(),
  syncDiagnostics = null,
} = {}) {
  const missing = new Set(Array.from(missingCollections || []));
  const errors = collectionErrors instanceof Map
    ? collectionErrors
    : new Map(Object.entries(collectionErrors || {}));
  const syncCollections = syncDiagnostics?.collections || {};
  const problemCollections = new Set();
  const startingCollections = new Set();
  const details = [];

  for (const name of requiredCollections) {
    const sync = syncCollections[name] || null;
    const localError = errors.get(name);
    if (missing.has(name)) {
      problemCollections.add(name);
      details.push(`${name}: collection missing`);
    }
    if (localError) {
      problemCollections.add(name);
      details.push(`${name}: ${errorToText(localError)}`);
    }
    const status = String(sync?.status || '').toLowerCase();
    const connectionStatus = String(sync?.connectionStatus || '').toLowerCase();
    if (sync?.lastError) {
      problemCollections.add(name);
      details.push(`${name}: ${syncErrorText(sync.lastError)}`);
    } else if (sync?.lastLifecycleEvent && isProblemLifecycleEvent(sync.lastLifecycleEvent)) {
      problemCollections.add(name);
      details.push(`${name}: ${syncErrorText(sync.lastLifecycleEvent)}`);
    } else if (['failed', 'error'].includes(status) || ['failed', 'error'].includes(connectionStatus)) {
      problemCollections.add(name);
      details.push(`${name}: ${status || connectionStatus}`);
    }
    if (!problemCollections.has(name) && (!sync || ['pending', 'starting', 'connecting', 'reconnecting'].includes(status) || ['pending', 'starting', 'connecting', 'reconnecting'].includes(connectionStatus))) {
      startingCollections.add(name);
    }
  }

  return {
    hasFailure: problemCollections.size > 0,
    isStarting: problemCollections.size === 0 && startingCollections.size > 0,
    problemCollections: [...problemCollections],
    startingCollections: [...startingCollections],
    detail: details.join(' · '),
  };
}

function syncErrorText(error) {
  if (!error) return '';
  if (typeof error === 'string') return error;
  return error.message || error.reason || error.code || JSON.stringify(error);
}

function isProblemLifecycleEvent(event) {
  const severity = String(event?.severity || '').toLowerCase();
  const code = String(event?.code || '').toLowerCase();
  return ['error', 'fatal'].includes(severity) || code.includes('timeout');
}

function errorToText(error) {
  if (!error) return '';
  if (typeof error === 'string') return error;
  return error.message || String(error);
}

function parseConversationDeepLink() {
  const query = String(location.hash || '').split('?')[1] || '';
  const params = new URLSearchParams(query);
  return {
    campaign_id: cleanParam(params.get('campaign_id')),
    engagement_id: cleanParam(params.get('engagement_id')),
    outbound_message_id: cleanParam(params.get('outbound_message_id') || params.get('message_id')),
    thread_key: cleanParam(params.get('thread_key')),
    message_key: cleanParam(params.get('message_key')),
    account_key: cleanParam(params.get('account_key')),
    channel: normalizeConversationChannel(params.get('channel')),
    applied: false,
    resolved: false,
  };
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

async function findOneJson(collection, id) {
  if (!collection || !id) return null;
  const doc = await collection.findOne(id).exec();
  return doc?.toJSON?.() || null;
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
  return t(`channel${channel[0].toUpperCase()}${channel.slice(1)}`, CHANNEL_LABEL_FALLBACK[channel] || channel);
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
    case 'queued': return t('statusQueued', 'in Warteschlange');
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

function shortenId(value) {
  if (!value) return '';
  const text = String(value);
  if (text.length <= 16) return text;
  return `${text.slice(0, 8)}…${text.slice(-4)}`;
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

function healthStateFor(account) {
  const latest = Math.max(isoToMs(account?.last_inbound_ok_at), isoToMs(account?.last_outbound_ok_at));
  if (!latest) return { state: 'bad', tooltip: 'never' };
  const age = Date.now() - latest;
  if (age < ACCOUNT_HEALTH_WARN_MS) return { state: 'ok', tooltip: '' };
  if (age < ACCOUNT_HEALTH_BAD_MS) return { state: 'warn', tooltip: '' };
  return { state: 'bad', tooltip: '' };
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

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${STYLE_BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

export const __conversationsTestHooks = {
  buildConversationDataDiagnostics,
  conversationEmptyState,
  filterBuckets,
  hasActiveListFilters,
  hasLocalCommunicationData,
};
