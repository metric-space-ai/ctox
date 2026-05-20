import { loadModuleMessages } from '../../shared/i18n.js';

// This module reads the canonical CTOX channel projection (mirror of the
// communication_* tables in runtime/ctox.sqlite3). Per-channel threads are the
// source-of-truth shape; the contact-centric list and channel-tabbed detail
// view bucket them client-side by participant_keys_json.

const STYLE_BUILD = '20260520-conversations3';
const SUPPORTED_CHANNELS = ['whatsapp', 'email', 'jami', 'teams', 'meeting'];
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
    searchPlaceholder: 'Kontakt, Inhalt, Account…',
    emptyListTitle: 'Keine Konversationen',
    emptyListBody: 'Hier erscheinen Kommunikationen, die CTOX über WhatsApp, E-Mail, Jami, MS Teams oder Meeting führt.',
    emptyDetailTitle: 'Keine Konversation ausgewählt',
    emptyDetailBody: 'Wähle links eine Konversation, um die Timeline aus CTOX-Sicht zu sehen.',
    rightEmptyBody: 'Kontaktdaten und verknüpfte Datensätze erscheinen hier.',
    cardLabelContact: 'Kontakt',
    cardLabelChannels: 'Aktive Channels',
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
    searchPlaceholder: 'Contact, content, account…',
    emptyListTitle: 'No conversations',
    emptyListBody: 'Communications CTOX has on WhatsApp, Email, Jami, MS Teams, or Meeting appear here.',
    emptyDetailTitle: 'No conversation selected',
    emptyDetailBody: 'Pick a conversation on the left to view the CTOX-side timeline.',
    rightEmptyBody: 'Contact details and linked records appear here.',
    cardLabelContact: 'Contact',
    cardLabelChannels: 'Active channels',
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

  const messages = await loadModuleMessages(import.meta.url, ctx.locale, FALLBACK_LABELS);
  const t = (key, fallback) => messages[key] ?? fallback ?? key;

  applyStaticLabels(root, t);

  const refs = collectRefs(root);
  const threadsCollection = ctx.db?.collection?.('communication_threads') || null;
  const messagesCollection = ctx.db?.collection?.('communication_messages') || null;
  const accountsCollection = ctx.db?.collection?.('communication_accounts') || null;

  const view = {
    channel: 'all',
    account: '',
    direction: 'any',
    dateRange: 'any',
    search: '',
    threads: [],
    accountsById: new Map(),
    buckets: [],
    selectedBucketKey: null,
    selectedChannel: ALL_CHANNELS_TAB,
    timelineMessages: [],
    timelineTotal: 0,
    timelineLimit: MESSAGE_PAGE_SIZE,
  };

  const cleanups = [];

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

  await Promise.all([loadAccounts(), loadThreads()]);
  rebuildBuckets();
  populateAccountFilter();
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

  return () => {
    for (const dispose of cleanups) {
      try { dispose?.(); } catch (error) { console.error('[conversations] cleanup:', error); }
    }
  };

  // ----- data loaders -----

  async function loadThreads() {
    if (!threadsCollection) {
      view.threads = [];
      return;
    }
    try {
      const docs = await threadsCollection.find().exec();
      view.threads = docs
        .map((doc) => doc.toJSON())
        .sort((a, b) => compareIsoDesc(a.last_message_at, b.last_message_at));
    } catch (error) {
      console.error('[conversations] loadThreads failed:', error);
      view.threads = [];
    }
  }

  async function loadAccounts() {
    if (!accountsCollection) {
      view.accountsById = new Map();
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
    } catch (error) {
      console.error('[conversations] loadAccounts failed:', error);
      view.accountsById = new Map();
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
    } catch (error) {
      console.error('[conversations] loadTimeline failed:', error);
      view.timelineMessages = [];
      view.timelineTotal = 0;
    }
    return view.timelineMessages;
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
  }

  // ----- renderers -----

  function renderList() {
    const filtered = filterBuckets(view.buckets, view, t);
    refs.threadList.replaceChildren();
    updateChannelCounts(view.buckets);
    if (!filtered.length) {
      refs.emptyList.hidden = false;
      if (view.selectedBucketKey) {
        view.selectedBucketKey = null;
        view.selectedChannel = ALL_CHANNELS_TAB;
        renderDetail();
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
    refs.timeline.scrollTop = refs.timeline.scrollHeight;
  }

  function buildMessageBubble(msg) {
    const el = document.createElement('article');
    el.className = 'conv-message';
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
    renderFolderCard(bucket);
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
}

// ----- helpers (pure / DOM-agnostic) -----

function collectRefs(root) {
  return {
    threadList: root.querySelector('[data-conv-thread-list]'),
    search: root.querySelector('[data-conv-search]'),
    channelFilterButtons: Array.from(root.querySelectorAll('[data-conv-channel-filters] [data-channel]')),
    accountFilter: root.querySelector('[data-conv-account-filter]'),
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
    rightDetail: root.querySelector('[data-conv-right-detail]'),
    contactAvatar: root.querySelector('[data-conv-contact-avatar]'),
    contactName: root.querySelector('[data-conv-contact-name]'),
    contactHandle: root.querySelector('[data-conv-contact-handle]'),
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
    btn.addEventListener('click', () => {
      const channel = btn.dataset.channel || 'all';
      if (view.channel === channel) return;
      view.channel = channel;
      for (const other of buttons) other.classList.toggle('is-active', other === btn);
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
