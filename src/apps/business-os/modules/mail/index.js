import { loadModuleMessages } from '../../shared/i18n.js';
import { openUniversalImporter } from '../../shared/universal-importer.js';
import { createMailContentEditor } from './editor/mail-content-editor.mjs';
import {
  appendMailContentRevision,
  applyContentRevisionToPending,
  buildMailContentRevision,
  buildMailGroupRecord,
  deriveMailGroupProgress,
  emailGroupDescriptor,
  mailGroupTemplate,
} from './mail-group-model.mjs';

const STYLE_BUILD = '20260806-mail-v5';
const MAIL_PAGE_SIZE = 50;
const TERMINAL_COMMAND_STATUSES = new Set(['completed', 'failed', 'cancelled', 'blocked']);
const DRAFT_SEND_STATUSES = new Set([
  '',
  'not_scheduled',
  'draft',
  'awaiting_approval',
  'approved_not_sent',
  'paused',
]);

const FALLBACK_LABELS = {
  de: {
    workspace: 'Arbeitsbereich',
    mail: 'Mail',
    compose: 'Neue Mail',
    mailbox: 'Postfach',
    allMailboxes: 'Alle sichtbaren Postfächer',
    noMailbox: 'Kein E-Mail-Postfach verbunden',
    search: 'Mails durchsuchen',
    folders: 'Ordner',
    inbox: 'Posteingang',
    unread: 'Ungelesen',
    sent: 'Gesendet',
    drafts: 'Entwürfe',
    groups: 'E-Mail-Gruppen',
    noGroups: 'Noch keine E-Mail-Gruppen',
    messages: 'Nachrichten',
    thread: 'Thread',
    campaign: 'E-Mail-Gruppe',
    emptyTitle: 'Keine E-Mails',
    emptyBody: 'Nachrichten erscheinen nach der ersten Synchronisierung.',
    syncingTitle: 'Mail wird synchronisiert',
    syncingBody: 'Postfächer und Nachrichten werden gerade geladen.',
    noSelectionTitle: 'Keine Mail ausgewählt',
    noSelectionBody: 'Wähle eine E-Mail oder E-Mail-Gruppe aus.',
    reply: 'Antworten',
    openOutbound: 'In Outbound öffnen',
    requestApproval: 'Zur Freigabe',
    approve: 'Freigeben',
    send: 'Senden',
    pause: 'Pausieren',
    resume: 'Fortsetzen',
    from: 'Von',
    to: 'An',
    group: 'E-Mail-Gruppe',
    ungrouped: 'Neue Gruppe automatisch',
    subject: 'Betreff',
    message: 'Nachricht',
    saveDraft: 'Als Entwurf speichern',
    savedDraft: 'Entwurf gespeichert.',
    approvalRequested: 'Entwurf wurde zur Freigabe eingereicht.',
    approved: 'Nachricht wurde freigegeben.',
    queued: 'Nachricht wurde in die Mailserver-Queue eingereiht.',
    missingRecipient: 'Bitte eine gültige Empfängeradresse eingeben.',
    missingSender: 'Bitte ein Absenderpostfach auswählen.',
    missingContent: 'Betreff oder Nachricht darf nicht leer sein.',
    commandFailed: 'Die Mail-Aktion ist fehlgeschlagen.',
    newGroup: 'Neue Gruppe',
    groupName: 'Name',
    groupDescription: 'Beschreibung',
    createGroup: 'Gruppe erstellen',
    groupCreated: 'Mailgruppe wurde erstellt.',
    groupNameRequired: 'Bitte einen Gruppennamen eingeben.',
    total: 'Gesamt',
    awaiting: 'Freigabe',
    failed: 'Fehler',
    active: 'Aktiv',
    paused: 'Pausiert',
    refreshing: 'Wird aktualisiert…',
    accountShared: 'Geteilt',
    accountPersonal: 'Zugewiesen',
    manageMailboxes: 'Postfächer verwalten',
    administration: 'Administration',
    mailserverDomains: 'Mailserver-Domains',
    noDomains: 'Keine Domain konfiguriert',
    createMailbox: 'Postfach anlegen',
    emailAddress: 'E-Mail-Adresse',
    ownerUser: 'Nutzer-ID oder Login',
    initialPassword: 'Initialpasswort',
    existingMailboxes: 'Vorhandene Postfächer',
    mailboxSecurity: 'Das Passwort wird nach der Einrichtung aus dem Command entfernt.',
    reload: 'Neu laden',
    loadingMailboxes: 'Mailserver-Konfiguration wird geladen…',
    noManagedMailboxes: 'Noch keine Mailserver-Postfächer.',
    assignedTo: 'Zugeordnet zu',
    unassigned: 'Nicht zugeordnet',
    delete: 'Löschen',
    confirmDelete: 'Wirklich löschen',
    cancel: 'Abbrechen',
    mailboxRequired: 'Bitte eine gültige E-Mail-Adresse und ein Passwort eingeben.',
    mailboxCreated: 'Postfach wurde angelegt und dem Nutzer zugeordnet.',
    mailboxDeleted: 'Postfach wurde gelöscht.',
    mailboxAdminDenied: 'Nur Administratoren dürfen Postfächer verwalten.',
  },
  en: {
    workspace: 'Workspace',
    mail: 'Mail',
    compose: 'New email',
    mailbox: 'Mailbox',
    allMailboxes: 'All visible mailboxes',
    noMailbox: 'No email mailbox connected',
    search: 'Search mail',
    folders: 'Folders',
    inbox: 'Inbox',
    unread: 'Unread',
    sent: 'Sent',
    drafts: 'Drafts',
    groups: 'Email groups',
    noGroups: 'No email groups yet',
    messages: 'messages',
    thread: 'Thread',
    campaign: 'Email group',
    emptyTitle: 'No email',
    emptyBody: 'Messages appear after the first synchronization.',
    syncingTitle: 'Syncing mail',
    syncingBody: 'Mailboxes and messages are loading.',
    noSelectionTitle: 'No email selected',
    noSelectionBody: 'Select an email or email group.',
    reply: 'Reply',
    openOutbound: 'Open in Outbound',
    requestApproval: 'Request approval',
    approve: 'Approve',
    send: 'Send',
    pause: 'Pause',
    resume: 'Resume',
    from: 'From',
    to: 'To',
    group: 'Email group',
    ungrouped: 'Create a new group automatically',
    subject: 'Subject',
    message: 'Message',
    saveDraft: 'Save draft',
    savedDraft: 'Draft saved.',
    approvalRequested: 'Draft was submitted for approval.',
    approved: 'Message approved.',
    queued: 'Message queued in the mailserver.',
    missingRecipient: 'Enter a valid recipient address.',
    missingSender: 'Select a sender mailbox.',
    missingContent: 'Subject or message must not be empty.',
    commandFailed: 'The mail action failed.',
    newGroup: 'New group',
    groupName: 'Name',
    groupDescription: 'Description',
    createGroup: 'Create group',
    groupCreated: 'Mail group created.',
    groupNameRequired: 'Enter a group name.',
    total: 'Total',
    awaiting: 'Approval',
    failed: 'Failed',
    active: 'Active',
    paused: 'Paused',
    refreshing: 'Refreshing…',
    accountShared: 'Shared',
    accountPersonal: 'Assigned',
    manageMailboxes: 'Manage mailboxes',
    administration: 'Administration',
    mailserverDomains: 'Mailserver domains',
    noDomains: 'No domain configured',
    createMailbox: 'Create mailbox',
    emailAddress: 'Email address',
    ownerUser: 'User ID or login',
    initialPassword: 'Initial password',
    existingMailboxes: 'Existing mailboxes',
    mailboxSecurity: 'The password is removed from the command after setup.',
    reload: 'Reload',
    loadingMailboxes: 'Loading mailserver configuration…',
    noManagedMailboxes: 'No mailserver mailboxes yet.',
    assignedTo: 'Assigned to',
    unassigned: 'Not assigned',
    delete: 'Delete',
    confirmDelete: 'Confirm deletion',
    cancel: 'Cancel',
    mailboxRequired: 'Enter a valid email address and a password.',
    mailboxCreated: 'Mailbox created and assigned to the user.',
    mailboxDeleted: 'Mailbox deleted.',
    mailboxAdminDenied: 'Only administrators may manage mailboxes.',
  },
};

export async function mount(ctx) {
  await ensureStyles();
  const htmlUrl = new URL('./index.html', import.meta.url);
  const version = String(import.meta.url).split('?v=')[1] || STYLE_BUILD;
  htmlUrl.searchParams.set('v', version);
  ctx.host.innerHTML = await fetch(htmlUrl).then((response) => response.text());
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-mail-root]');
  if (!root) throw new Error('mail: root missing after fragment mount');

  const messages = await loadModuleMessages(import.meta.url, ctx.locale, FALLBACK_LABELS);
  const t = (key, fallback) => messages[key] ?? fallback ?? key;
  applyStaticLabels(root, t);

  const refs = collectRefs(root);
  renderActionIcons(ctx, refs);
  const collections = Object.fromEntries([
    'business_commands',
    'business_module_catalog',
    'business_users',
    'communication_accounts',
    'communication_threads',
    'communication_messages',
    'outbound_campaigns',
    'outbound_pipeline_items',
    'outbound_engagements',
    'outbound_messages',
    'outbound_approvals',
  ].map((name) => [name, ctx.db?.collection?.(name) || null]));

  const view = {
    accounts: [],
    threads: [],
    communicationMessages: [],
    campaigns: [],
    engagements: [],
    outboundMessages: [],
    approvals: [],
    commands: [],
    routeDestinations: [],
    users: [],
    accountKey: '',
    scopeType: 'queue',
    scopeId: 'inbound',
    search: '',
    leftGrammar: { search: '', view: 'cards', band: 'queues', filters: { account: 'all', sort: 'recent' } },
    listGrammar: { search: '', view: 'cards', band: 'all', filters: { status: 'all', sort: 'recent' } },
    selectedKind: '',
    selectedId: '',
    loading: true,
    disposed: false,
    readiness: null,
    refreshTimer: null,
    busy: false,
    mailserver: {
      open: false,
      loading: false,
      domains: [],
      users: [],
      runtimeConfig: {},
      runtimeStatus: {},
      health: {},
      error: '',
      message: '',
      pendingDelete: '',
      pendingDomainDelete: '',
    },
    pendingSeriesHandoff: null,
    selectedRecords: new Set(),
    page: 0,
    pageSize: MAIL_PAGE_SIZE,
    route: { open: false, busy: false, error: '', status: '', lastCommandId: '' },
    contentEditor: null,
    contentGroupId: '',
  };
  const cleanups = [];

  const savedLeft = Number(ctx.storageScope?.get?.('ctox.mail.layout.leftWidth') || 300);
  const savedRight = Number(ctx.storageScope?.get?.('ctox.mail.layout.rightWidth') || 480);
  root.style.setProperty('--mail-left-width', `${clamp(savedLeft, 250, 420)}px`);
  root.style.setProperty('--mail-right-width', `${clamp(savedRight, 300, 620)}px`);

  wireEvents();
  wireCollectionSubscriptions();
  wireReadiness();
  await refreshData();
  applyDeepLink();
  render();

  return () => {
    view.disposed = true;
    void view.contentEditor?.destroy?.();
    if (view.refreshTimer) window.clearTimeout(view.refreshTimer);
    for (const cleanup of cleanups) {
      try { cleanup?.(); } catch (error) { console.warn('[mail] cleanup failed', error); }
    }
  };

  function wireEvents() {
    refs.account?.addEventListener('change', () => {
      view.accountKey = refs.account.value === 'all' ? '' : refs.account.value;
      view.selectedKind = '';
      view.selectedId = '';
      render();
    });
    refs.compose?.addEventListener('click', () => {
      closeMobileNavigation();
      openComposer();
    });
    refs.importButtons.forEach((button) => button?.addEventListener('click', openMailImporter));
    refs.exportButtons.forEach((button) => button?.addEventListener('click', exportVisibleMail));
    refs.settings?.addEventListener('click', openMailboxAdmin);
    refs.openNav?.addEventListener('click', openMobileNavigation);
    refs.closeNav?.addEventListener('click', closeMobileNavigation);
    refs.closeDetail?.addEventListener('click', closeDetail);
    refs.closeComposer?.addEventListener('click', closeComposer);
    refs.saveDraft?.addEventListener('click', () => submitComposer('draft'));
    refs.requestApproval?.addEventListener('click', () => submitComposer('approval'));
    refs.sendNow?.addEventListener('click', () => submitComposer('send'));
    refs.newGroup?.addEventListener('click', openGroupEditor);
    refs.closeGroup?.addEventListener('click', closeGroupEditor);
    refs.createGroup?.addEventListener('click', createGroup);
    refs.groupTemplate?.addEventListener('change', applyGroupTemplateDefaults);
    refs.closeContentEditor?.addEventListener('click', closeGroupContentEditor);
    refs.saveContent?.addEventListener('click', saveGroupContent);
    refs.closeMailboxAdmin?.addEventListener('click', closeMailboxAdmin);
    refs.reloadMailboxes?.addEventListener('click', loadMailserverConfig);
    refs.serverForm?.addEventListener('submit', saveMailserverRuntime);
    refs.domainForm?.addEventListener('submit', saveMailserverDomain);
    refs.toggleServer?.addEventListener('click', toggleMailserverRuntime);
    refs.mailboxForm?.addEventListener('submit', createMailbox);
    refs.selectPage?.addEventListener('change', togglePageSelection);
    refs.selectFiltered?.addEventListener('click', selectAllFiltered);
    refs.bulkRoute?.addEventListener('click', openRouteDrawer);
    refs.bulkApproval?.addEventListener('click', () => runBulkOutboundAction('request-approval'));
    refs.bulkSend?.addEventListener('click', () => runBulkOutboundAction('send'));
    refs.bulkExport?.addEventListener('click', exportSelectedMail);
    refs.clearSelection?.addEventListener('click', clearSelection);
    refs.closeRoute?.addEventListener('click', closeRouteDrawer);
    refs.cancelRoute?.addEventListener('click', closeRouteDrawer);
    refs.confirmRoute?.addEventListener('click', confirmRoute);
    refs.prevPage?.addEventListener('click', () => changePage(-1));
    refs.nextPage?.addEventListener('click', () => changePage(1));
    refs.leftPane?.addEventListener('ctox-pane-grammar-change', (event) => {
      if (event.target !== refs.leftPane) return;
      view.leftGrammar = normalizePaneGrammar(event.detail, view.leftGrammar);
      view.accountKey = view.leftGrammar.filters.account === 'all' ? '' : view.leftGrammar.filters.account;
      view.page = 0;
      renderNavigation();
      renderList();
    });
    refs.listPane?.addEventListener('ctox-pane-grammar-change', (event) => {
      if (event.target !== refs.listPane) return;
      view.listGrammar = normalizePaneGrammar(event.detail, view.listGrammar);
      view.search = view.listGrammar.search;
      view.page = 0;
      renderList();
    });

    // One-button view switch. Bound on the button itself, never delegated: the
    // shell grammar re-renders the icon on the same click, which detaches the
    // <svg> the click originated in, so `event.target.closest()` would find
    // nothing by the time the event reached the module root.
    refs.leftPane?.querySelector('[data-mail-view-toggle]')?.addEventListener('click', () => {
      view.leftGrammar = { ...view.leftGrammar, view: view.leftGrammar.view === 'list' ? 'cards' : 'list' };
      renderNavigation();
    });
    refs.listPane?.querySelector('[data-mail-view-toggle]')?.addEventListener('click', () => {
      view.listGrammar = { ...view.listGrammar, view: view.listGrammar.view === 'list' ? 'cards' : 'list' };
      view.page = 0;
      renderList();
    });

    root.addEventListener('click', async (event) => {
      const recordSelector = event.target.closest('[data-mail-select-record]');
      if (recordSelector) {
        event.stopPropagation();
        toggleRecordSelection(recordSelector.dataset.mailSelectRecord || '');
        return;
      }
      const scope = event.target.closest('[data-mail-scope]');
      if (scope) {
        view.scopeType = scope.dataset.mailScope;
        view.scopeId = scope.dataset.mailScopeId || '';
        view.selectedKind = '';
        view.selectedId = '';
        view.page = 0;
        clearSelection();
        closeRouteDrawer();
        concealInspector();
        if (view.scopeType === 'campaign') revealInspector('detail');
        closeMobileNavigation();
        renderNavigationSelection();
        renderList();
        renderDetail();
        return;
      }
      const record = event.target.closest('[data-mail-record-kind]');
      if (record) {
        view.selectedKind = record.dataset.mailRecordKind || '';
        view.selectedId = record.dataset.mailRecordId || '';
        revealInspector('detail');
        renderListSelection();
        renderDetail();
        return;
      }
      const action = event.target.closest('[data-mail-action]');
      if (action) {
        await handleDetailAction(action.dataset.mailAction, action.dataset.mailId || '');
        return;
      }
      const mailboxAction = event.target.closest('[data-mailbox-action]');
      if (mailboxAction) {
        await handleMailboxAction(mailboxAction.dataset.mailboxAction, mailboxAction.dataset.mailboxUsername || '');
        return;
      }
      const domainAction = event.target.closest('[data-mail-domain-action]');
      if (domainAction) {
        await handleDomainAction(domainAction.dataset.mailDomainAction, domainAction.dataset.mailDomainName || '');
      }
    });
    root.addEventListener('keydown', (event) => {
      if (!['Enter', ' '].includes(event.key) || event.target.closest('input, textarea, select, button')) return;
      const record = event.target.closest('[data-mail-record-kind]');
      if (!record) return;
      event.preventDefault();
      record.click();
    });

    const hashHandler = () => {
      if (!String(location.hash || '').startsWith('#mail')) return;
      applyDeepLink();
      render();
    };
    window.addEventListener('hashchange', hashHandler);
    cleanups.push(() => window.removeEventListener('hashchange', hashHandler));
  }

  function wireCollectionSubscriptions() {
    for (const [name, collection] of Object.entries(collections)) {
      if (!collection?.$) continue;
      const subscription = collection.$.subscribe(() => scheduleRefresh());
      cleanups.push(() => subscription.unsubscribe?.());
    }
  }

  function wireReadiness() {
    if (typeof ctx.sync?.collectionReadiness === 'function') {
      view.readiness = ctx.sync.collectionReadiness('communication_threads') || null;
    }
    if (typeof ctx.sync?.subscribeCollectionReadiness === 'function') {
      const unsubscribe = ctx.sync.subscribeCollectionReadiness('communication_threads', (snapshot) => {
        view.readiness = snapshot || null;
        if (!view.disposed) renderList();
      });
      cleanups.push(() => unsubscribe?.());
    }
  }

  function scheduleRefresh() {
    if (view.disposed) return;
    if (view.refreshTimer) window.clearTimeout(view.refreshTimer);
    view.refreshTimer = window.setTimeout(async () => {
      view.refreshTimer = null;
      if (view.disposed) return;
      await refreshData();
      if (!view.disposed) render();
    }, 120);
  }

  async function refreshData() {
    const [commands, catalogs, users, accounts, threads, communicationMessages, campaigns, engagements, outboundMessages, approvals] = await Promise.all([
      readAll(collections.business_commands),
      readAll(collections.business_module_catalog),
      readAll(collections.business_users),
      readAll(collections.communication_accounts),
      readAll(collections.communication_threads),
      readAll(collections.communication_messages),
      readAll(collections.outbound_campaigns),
      readAll(collections.outbound_engagements),
      readAll(collections.outbound_messages),
      readAll(collections.outbound_approvals),
    ]);
    if (view.disposed) return;
    view.commands = commands.filter((command) => !isDeleted(command));
    view.routeDestinations = routeDestinationsFromCatalog(catalogs, ctx.permissions);
    view.users = users.filter((user) => !isDeleted(user) && user.active !== false).sort((a, b) => userDisplayName(a).localeCompare(userDisplayName(b)));
    view.accounts = visibleEmailAccounts(accounts, ctx.session?.user || {});
    view.threads = threads.filter((thread) => thread.channel === 'email' && !isDeleted(thread));
    view.communicationMessages = communicationMessages.filter((message) => message.channel === 'email' && !isDeleted(message));
    view.campaigns = visibleMailCampaigns(campaigns, ctx.session?.user || {}, view.accounts, outboundMessages);
    view.engagements = engagements.filter((engagement) => !isDeleted(engagement));
    view.outboundMessages = outboundMessages.filter((message) => (
      !isDeleted(message) && (!message.channel || message.channel === 'email')
    )).sort(sortUpdatedDesc);
    view.approvals = approvals.filter((approval) => !isDeleted(approval));
    if (view.accountKey && !view.accounts.some((account) => account.account_key === view.accountKey)) {
      view.accountKey = '';
    }
    view.loading = false;
  }

  function render() {
    renderAccounts();
    renderNavigation();
    renderList();
    renderDetail();
    renderMailboxAdmin();
    if (view.pendingSeriesHandoff) {
      const seed = view.pendingSeriesHandoff;
      view.pendingSeriesHandoff = null;
      openComposer(seed);
    }
  }

  function renderAccounts() {
    if (!refs.account) return;
    const current = view.accountKey;
    refs.account.innerHTML = '';
    refs.account.append(option('all', view.accounts.length ? t('allMailboxes', 'Alle sichtbaren Postfächer') : t('noMailbox', 'Kein E-Mail-Postfach verbunden')));
    for (const account of view.accounts) {
      const profile = accountProfile(account);
      const kind = accountOwnedByCurrentUser(account, ctx.session?.user || {})
        ? t('accountPersonal', 'Persönlich')
        : t('accountShared', 'Gemeinsam');
      refs.account.append(option(account.account_key, `${account.address || account.account_key} · ${kind}`));
      void profile;
    }
    refs.account.value = current || 'all';
    refs.account.disabled = view.accounts.length === 0;
    if (refs.mailboxOwner) {
      const selectedOwner = refs.mailboxOwner.value;
      refs.mailboxOwner.innerHTML = `<option value="">— ${escapeHtml(t('selectPerson', 'Person wählen'))} —</option>${view.users.map((user) => `<option value="${escapeAttribute(user.id || user.user_id)}">${escapeHtml(userDisplayName(user))}</option>`).join('')}`;
      if (selectedOwner && view.users.some((user) => (user.id || user.user_id) === selectedOwner)) refs.mailboxOwner.value = selectedOwner;
    }
    populateComposerSelects();
  }

  function renderNavigation() {
    const visibleGroups = view.campaigns.filter((campaign) => !campaign.payload?.hidden_in_mail_groups);
    const queueRows = mailQueueDefinitions({
      threads: filteredAccountThreads(),
      outboundMessages: filteredAccountOutboundMessages(),
      communicationMessages: view.communicationMessages,
      commands: view.commands,
      t,
    });
    writePaneCounts(refs.leftPane, { queues: queueRows.length, campaigns: visibleGroups.length });
    refs.leftPane.dataset.mailView = view.leftGrammar.view;
    syncViewToggleButton(ctx, refs.leftPane, view.leftGrammar.view, t);
    const search = view.leftGrammar.search;
    const sort = view.leftGrammar.filters.sort || 'recent';
    const sources = view.leftGrammar.band === 'campaigns'
      ? visibleGroups.map((campaign) => {
        const stats = campaignStats(campaign.id, view.outboundMessages);
        const descriptor = emailGroupDescriptor(campaign);
        return {
          id: campaign.id,
          title: campaign.name || t('campaign', 'E-Mail-Gruppe'),
          meta: `${descriptor.kind} · ${stats.done} von ${stats.total} erledigt`,
          count: stats.percent,
          countLabel: `${stats.percent} %`,
          updatedAt: timeOf(campaign.updated_at_ms || campaign.updated_at),
        };
      })
      : queueRows;
    let rows = sources.filter((item) => !search || `${item.title} ${item.meta}`.toLowerCase().includes(search));
    rows = [...rows].sort((a, b) => sort === 'name'
      ? a.title.localeCompare(b.title)
      : sort === 'count'
        ? b.count - a.count
        : Number(b.updatedAt || 0) - Number(a.updatedAt || 0));
    // Shards carry title plus meta line; the list is one dense line per entry
    // with a single short meta on the right.
    const scopeAsList = view.leftGrammar.view === 'list';
    refs.scopeList.innerHTML = rows.length ? rows.map((item) => {
      const scopeType = view.leftGrammar.band === 'campaigns' ? 'campaign' : 'queue';
      const shape = scopeAsList ? 'mail-scope-card--line' : 'mail-scope-card--shard';
      const meta = scopeAsList ? '' : `<span class="mail-scope-meta">${escapeHtml(item.meta)}</span>`;
      return `<button class="mail-scope-card ${shape}${view.scopeType === scopeType && view.scopeId === item.id ? ' is-active' : ''}" type="button" data-mail-scope="${scopeType}" data-mail-scope-id="${escapeAttribute(item.id)}" data-context-record-id="${escapeAttribute(item.id)}" data-context-record-type="${scopeType}" data-context-record-label="${escapeAttribute(item.title)}" data-context-label="${escapeAttribute(item.title)}">
        <span class="mail-scope-title">${escapeHtml(item.title)}</span>${meta}<span class="mail-scope-count">${escapeHtml(item.countLabel ?? item.count)}</span>
      </button>`;
    }).join('') : `<div class="ctox-empty"><span>${escapeHtml(view.leftGrammar.band === 'campaigns' ? t('noGroups', 'Noch keine E-Mail-Gruppen') : t('noResults', 'Keine passenden Queues'))}</span></div>`;
    renderNavigationSelection();
    const groupsVisible = view.leftGrammar.band === 'campaigns';
    refs.navigationTitle.textContent = groupsVisible ? t('groups', 'E-Mail-Gruppen') : 'E-Mail-Queues';
    refs.newGroup.hidden = !groupsVisible;
    const footer = `${view.accounts.length} ${t('mailbox', 'Postfächer')} · ${visibleGroups.length} ${t('groups', 'E-Mail-Gruppen')}`;
    refs.sidebarFooter.textContent = footer;
    refs.leftPane.__ctoxPaneGrammar?.setFooter?.(footer);
  }

  function renderNavigationSelection() {
    for (const row of refs.scopeList.querySelectorAll('[data-mail-scope]')) {
      const selected = row.dataset.mailScope === view.scopeType
        && (row.dataset.mailScopeId || '') === view.scopeId;
      row.classList.toggle('is-active', selected);
      row.setAttribute('aria-selected', String(selected));
    }
  }

  function renderList() {
    const allRows = currentRecords();
    const maxPage = Math.max(0, Math.ceil(allRows.length / view.pageSize) - 1);
    view.page = Math.min(view.page, maxPage);
    const pageStart = view.page * view.pageSize;
    const rows = allRows.slice(pageStart, pageStart + view.pageSize);
    const counts = listBandCounts(scopeRecords(), view.commands);
    const scopeLabel = currentScopeLabel();
    refs.listKicker.textContent = view.scopeType === 'campaign' ? t('campaign', 'Kampagne') : t('mailbox', 'Postfach');
    refs.listTitle.textContent = scopeLabel;
    refs.listPane.dataset.mailView = view.listGrammar.view;
    syncViewToggleButton(ctx, refs.listPane, view.listGrammar.view, t);
    writePaneCounts(refs.listPane, counts);
    const range = allRows.length ? `${pageStart + 1}–${pageStart + rows.length} / ${allRows.length}` : '0';
    const footer = `${range} ${t('messages', 'Nachrichten')} · ${view.accountKey ? accountLabel(view.accountKey) : t('allMailboxes', 'alle Postfächer')}`;
    refs.listFooter.textContent = footer;
    refs.prevPage.disabled = view.page === 0;
    refs.nextPage.disabled = view.page >= maxPage;
    refs.listPane.__ctoxPaneGrammar?.setFooter?.(footer);
    refs.recordList.innerHTML = rows.map(renderRecordRow).join('');
    renderListSelection();
    renderBulkBar(rows, allRows);

    const shouldSync = view.loading || (view.readiness && view.readiness.ready === false);
    refs.listEmpty.hidden = allRows.length > 0;
    if (!allRows.length) {
      refs.emptyTitle.textContent = shouldSync ? t('syncingTitle', 'Mail wird synchronisiert') : t('emptyTitle', 'Keine E-Mails');
      refs.emptyBody.textContent = shouldSync ? t('syncingBody', 'Postfächer und Nachrichten werden gerade geladen.') : t('emptyBody', 'Nachrichten erscheinen nach der ersten Synchronisierung.');
    }
  }

  // Two genuinely different shapes for the same record:
  //   shard - bold subject plus one or two meta lines (sender, status, group,
  //           delivery progress) and generous padding;
  //   line  - exactly one dense row: subject and a single short meta (time).
  function renderRecordRow(record) {
    const asList = view.listGrammar.view === 'list';
    const shape = asList ? 'mail-record-row--line' : 'mail-record-row--shard';
    if (record.__kind === 'thread') {
      const latest = latestMessageForThread(record.thread_key, view.communicationMessages);
      const sender = latest?.direction === 'outbound'
        ? (latest.recipient_addresses_json?.[0] || record.participant_keys_json?.[0] || record.account_key)
        : (latest?.sender_display || latest?.sender_address || record.participant_keys_json?.[0] || record.account_key);
      const subject = latest?.subject || record.subject || '(Kein Betreff)';
      const preview = latest?.preview || latest?.body_text || '';
      const selectionKey = mailRecordKey(record);
      const route = routeCommandForRecord(record, view.commands);
      const unread = Number(record.unread_count || 0);
      const status = route ? routeTargetLabel(route) : (unread ? `${unread} neu` : t('inbound', 'Eingang'));
      const open = `<div class="mail-record-row ${shape}${unread > 0 ? ' is-unread' : ''}" role="option" tabindex="0" data-mail-record-kind="thread" data-mail-record-id="${escapeAttribute(record.thread_key)}" data-context-record-id="${escapeAttribute(record.thread_key)}" data-context-record-type="communication_thread" data-context-record-label="${escapeAttribute(subject)}" data-context-label="${escapeAttribute(subject)}">
        <input class="mail-record-select" type="checkbox" data-mail-select-record="${escapeAttribute(selectionKey)}" aria-label="${escapeAttribute(subject)} auswählen" ${view.selectedRecords.has(selectionKey) ? 'checked' : ''} />`;
      const time = `<span class="mail-record-time">${escapeHtml(formatRecordTime(record.last_message_at))}</span>`;
      if (asList) {
        return `${open}
        <span class="mail-record-subject">${escapeHtml(subject)}</span>
        ${time}
      </div>`;
      }
      const previewLine = compact(preview);
      return `${open}
        <span class="mail-record-copy"><span class="mail-record-subject">${escapeHtml(subject)}</span><span class="mail-record-meta">${escapeHtml(sender || '—')} · ${escapeHtml(status)}</span>${previewLine ? `<span class="mail-record-preview">${escapeHtml(previewLine)}</span>` : ''}</span>
        ${time}
      </div>`;
    }
    const campaign = view.campaigns.find((item) => item.id === record.campaign_id);
    const recipient = record.recipient_email || record.recipient_address_text || '—';
    const status = messageStatusLabel(record, t);
    const selectionKey = mailRecordKey(record);
    const route = routeCommandForRecord(record, view.commands);
    const displayStatus = route ? routeTargetLabel(route) : status;
    const subject = record.subject || '(Kein Betreff)';
    const open = `<div class="mail-record-row ${shape}" role="option" tabindex="0" data-mail-record-kind="outbound" data-mail-record-id="${escapeAttribute(record.id)}" data-context-record-id="${escapeAttribute(record.id)}" data-context-record-type="outbound_message" data-context-record-label="${escapeAttribute(record.subject || recipient)}" data-context-label="${escapeAttribute(record.subject || recipient)}">
      <input class="mail-record-select" type="checkbox" data-mail-select-record="${escapeAttribute(selectionKey)}" aria-label="${escapeAttribute(record.subject || recipient)} auswählen" ${view.selectedRecords.has(selectionKey) ? 'checked' : ''} />`;
    const time = `<span class="mail-record-time">${escapeHtml(formatRecordTime(record.updated_at_ms || record.created_at_ms))}</span>`;
    if (asList) {
      return `${open}
      <span class="mail-record-subject">${escapeHtml(subject)}</span>
      ${time}
    </div>`;
    }
    const progress = messageProgressModel(record);
    const progressIcons = progress.steps.map((step) => `<span class="mail-progress-step is-${escapeAttribute(step.state)}" title="${escapeAttribute(step.label)}" aria-label="${escapeAttribute(step.label)}">${mailActionIcon(ctx, step.icon, 11, 1.9)}</span>`).join('');
    const group = campaign?.name ? `${escapeHtml(campaign.name)} · ` : '';
    return `${open}
      <span class="mail-record-copy"><span class="mail-record-subject">${escapeHtml(subject)}</span><span class="mail-record-meta">${escapeHtml(recipient)} · ${group}${escapeHtml(status)}</span><span class="mail-record-progress" role="img" aria-label="${escapeAttribute(progress.ariaLabel)}"><span class="mail-progress-track"><i style="width:${progress.percent}%"></i></span><span class="mail-progress-steps">${progressIcons}</span><span class="mail-progress-label">${escapeHtml(displayStatus)}</span></span></span>
      ${time}
    </div>`;
  }

  function renderListSelection() {
    refs.recordList.querySelectorAll('[data-mail-record-kind]').forEach((row) => {
      row.classList.toggle('is-selected', row.dataset.mailRecordKind === view.selectedKind && row.dataset.mailRecordId === view.selectedId);
      const key = `${row.dataset.mailRecordKind}:${row.dataset.mailRecordId}`;
      const bulkSelected = view.selectedRecords.has(key);
      row.classList.toggle('is-bulk-selected', bulkSelected);
      const checkbox = row.querySelector('[data-mail-select-record]');
      if (checkbox) checkbox.checked = bulkSelected;
    });
  }

  function renderDetail() {
    if (view.selectedKind && view.selectedId && !view.route.open) revealInspector('detail');
    if (view.selectedKind === 'thread') {
      refs.detailTitle.textContent = t('thread', 'Thread');
      renderThreadDetail(view.selectedId);
      return;
    }
    if (view.selectedKind === 'outbound') {
      refs.detailTitle.textContent = t('message', 'Nachricht');
      renderOutboundDetail(view.selectedId);
      return;
    }
    if (view.scopeType === 'campaign') {
      refs.detailTitle.textContent = t('campaign', 'Kampagne');
      renderCampaignDetail(view.scopeId);
      return;
    }
    if (!view.route.open) concealInspector();
    refs.detailTitle.textContent = t('message', 'Nachricht');
    renderMissingDetail();
  }

  function renderThreadDetail(threadKey) {
    const thread = view.threads.find((item) => item.thread_key === threadKey);
    if (!thread) return renderMissingDetail();
    const timeline = view.communicationMessages
      .filter((message) => message.thread_key === threadKey)
      .sort((a, b) => timeOf(a.external_created_at) - timeOf(b.external_created_at));
    const participants = (thread.participant_keys_json || []).join(', ') || thread.account_key;
    const replyTo = [...timeline].reverse().find((message) => message.direction === 'inbound')?.sender_address || '';
    refs.detail.innerHTML = `<article class="mail-detail-shell" data-context-record-id="${escapeAttribute(threadKey)}" data-context-record-type="communication_thread" data-context-record-label="${escapeAttribute(thread.subject || '(Kein Betreff)')}" data-context-label="${escapeAttribute(thread.subject || '(Kein Betreff)')}">
      <header class="mail-detail-header">
        <div><span class="ctox-pane-kicker">${escapeHtml(t('thread', 'Thread'))}</span><h3>${escapeHtml(thread.subject || timeline.at(-1)?.subject || '(Kein Betreff)')}</h3><div class="mail-detail-participants">${escapeHtml(participants)}</div></div>
        <div class="mail-detail-actions"><button type="button" class="ctox-button is-primary" data-mail-action="reply" data-mail-id="${escapeAttribute(threadKey)}">${escapeHtml(t('reply', 'Antworten'))}</button><button type="button" class="ctox-button" data-mail-action="route" data-mail-id="${escapeAttribute(threadKey)}">${escapeHtml(t('routeToApp', 'An App routen'))}</button></div>
      </header>
      <div class="mail-thread-timeline">${timeline.length ? timeline.map(renderTimelineMessage).join('') : `<div class="ctox-empty"><span>${escapeHtml(t('emptyBody', 'Nachrichten erscheinen nach der ersten Synchronisierung.'))}</span></div>`}</div>
      <footer class="ctox-pane-footer"><span>${escapeHtml(replyTo || thread.account_key)}</span></footer>
    </article>`;
  }

  function renderTimelineMessage(message) {
    const outbound = message.direction === 'outbound';
    const party = outbound
      ? (message.recipient_addresses_json?.join(', ') || message.account_key)
      : (message.sender_display || message.sender_address || '—');
    return `<section class="mail-message${outbound ? ' is-outbound' : ''}">
      <div class="mail-message-head"><strong>${escapeHtml(party)}</strong><span>${escapeHtml(formatRecordTime(message.external_created_at))}</span></div>
      <div class="mail-message-body">${escapeHtml(message.body_text || message.preview || '')}</div>
    </section>`;
  }

  function renderOutboundDetail(messageId) {
    const message = view.outboundMessages.find((item) => item.id === messageId);
    if (!message) return renderMissingDetail();
    const campaign = view.campaigns.find((item) => item.id === message.campaign_id);
    const actions = [
      ...messageActions(message, t),
      { id: 'refresh-status', label: 'Status abrufen', primary: false },
      { id: 'route', label: t('routeToApp', 'An App routen'), primary: false },
    ]
      .map((action) => `<button type="button" class="ctox-button${action.primary ? ' is-primary' : ''}" data-mail-action="${escapeAttribute(action.id)}" data-mail-id="${escapeAttribute(message.id)}">${escapeHtml(action.label)}</button>`)
      .join('');
    refs.detail.innerHTML = `<article class="mail-detail-shell" data-context-record-id="${escapeAttribute(message.id)}" data-context-record-type="outbound_message" data-context-record-label="${escapeAttribute(message.subject || '(Kein Betreff)')}" data-context-label="${escapeAttribute(message.subject || '(Kein Betreff)')}">
      <header class="mail-detail-header">
        <div><span class="ctox-pane-kicker">${escapeHtml(campaign?.name || t('campaign', 'Kampagne'))}</span><h3>${escapeHtml(message.subject || '(Kein Betreff)')}</h3><div class="mail-detail-participants">${escapeHtml(message.sender_account_id || '')} → ${escapeHtml(message.recipient_email || message.recipient_address_text || '—')}</div></div>
        <div class="mail-detail-actions">${actions}</div>
      </header>
      <div class="mail-delivery-timeline">${messageEventTimeline(message).map((event) => `<div class="mail-delivery-event is-${escapeAttribute(event.state)}"><span></span><div><strong>${escapeHtml(event.label)}</strong><small>${escapeHtml(event.detail)}</small></div></div>`).join('')}</div>
      <div class="mail-thread-timeline">
        <section class="mail-message is-outbound"><div class="mail-message-head"><strong>${escapeHtml(message.recipient_email || '—')}</strong><span>${escapeHtml(messageStatusLabel(message, t))}</span></div><div class="mail-message-body">${escapeHtml(message.body_text || '')}</div></section>
      </div>
      <footer class="ctox-pane-footer"><span>${escapeHtml(messageStatusLabel(message, t))}</span></footer>
    </article>`;
  }

  function renderCampaignDetail(campaignId) {
    const campaign = view.campaigns.find((item) => item.id === campaignId);
    if (!campaign) return renderMissingDetail();
    const stats = campaignStats(campaignId, view.outboundMessages);
    const descriptor = emailGroupDescriptor(campaign);
    const status = String(campaign.status || 'active').toLowerCase();
    refs.detail.innerHTML = `<article class="mail-detail-shell" data-context-record-id="${escapeAttribute(campaign.id)}" data-context-record-type="outbound_campaign" data-context-record-label="${escapeAttribute(campaign.name)}" data-context-label="${escapeAttribute(campaign.name)}">
      <header class="mail-detail-header">
        <div><span class="ctox-pane-kicker">${escapeHtml(groupKindLabel(descriptor.kind))}</span><h3>${escapeHtml(campaign.name)}</h3><div class="mail-detail-participants">${escapeHtml(status === 'paused' ? t('paused', 'Pausiert') : t('active', 'Aktiv'))} · ${escapeHtml(descriptor.intakeMode === 'dynamic' ? 'dynamischer Eingang' : 'fester Umfang')}</div></div>
        <div class="mail-detail-actions">
          <button type="button" class="ctox-button is-primary" data-mail-action="compose-campaign" data-mail-id="${escapeAttribute(campaign.id)}">${escapeHtml(t('compose', 'Neue Mail'))}</button>
          <button type="button" class="ctox-button" data-mail-action="edit-group-content" data-mail-id="${escapeAttribute(campaign.id)}">${escapeHtml(groupContentLabel(descriptor.contentMode))}</button>
          <button type="button" class="ctox-button" data-mail-action="campaign-${status === 'paused' ? 'resume' : 'pause'}" data-mail-id="${escapeAttribute(campaign.id)}">${escapeHtml(status === 'paused' ? t('resume', 'Fortsetzen') : t('pause', 'Pausieren'))}</button>
        </div>
      </header>
      <div class="mail-campaign-overview">
        <div class="mail-group-assignment"><span>Auftrag</span><strong>${escapeHtml(campaign.objective || descriptor.objective)}</strong></div>
        <div class="mail-group-progress" aria-label="${escapeAttribute(`${stats.done} von ${stats.total} erledigt`)}"><div><strong>${stats.done} von ${stats.total} erledigt</strong><span>${stats.percent} %</span></div><progress value="${stats.done}" max="${Math.max(1, stats.total)}">${stats.percent} %</progress></div>
        <div class="mail-metric-strip">
          ${renderMetric(stats.total, t('total', 'Gesamt'))}
          ${renderMetric(stats.queued, 'In Queue')}
          ${renderMetric(stats.delivered, 'Zugestellt')}
          ${renderMetric(stats.opened, 'Öffnung erfasst')}
          ${renderMetric(stats.clicked, 'Link geklickt')}
          ${renderMetric(stats.failed, 'Fehler')}
        </div>
      </div>
      <footer class="ctox-pane-footer"><span>${escapeHtml(descriptor.accountKey || 'Absenderkonto noch nicht festgelegt')}</span><span>${stats.complete ? 'Auftrag abgeschlossen' : `${stats.open + stats.working} noch zu bearbeiten`}</span></footer>
    </article>`;
  }

  function renderMissingDetail() {
    refs.detail.innerHTML = `<div class="ctox-empty mail-detail-empty"><strong>${escapeHtml(t('noSelectionTitle', 'Keine Mail ausgewählt'))}</strong><span>${escapeHtml(t('noSelectionBody', 'Wähle eine Nachricht oder Kampagne aus.'))}</span></div>`;
  }

  async function handleDetailAction(action, id) {
    if (view.busy) return;
    if (action === 'edit-group-content') {
      await openGroupContentEditor(id);
      return;
    }
    if (action === 'reply') {
      const thread = view.threads.find((item) => item.thread_key === id);
      const timeline = view.communicationMessages.filter((message) => message.thread_key === id);
      const inbound = [...timeline].sort((a, b) => timeOf(b.external_created_at) - timeOf(a.external_created_at)).find((message) => message.direction === 'inbound');
      openComposer({
        to: inbound?.sender_address || thread?.participant_keys_json?.[0] || '',
        subject: replySubject(inbound?.subject || thread?.subject || ''),
        accountKey: thread?.account_key || '',
      });
      return;
    }
    if (action === 'route') {
      const record = view.threads.find((item) => item.thread_key === id)
        || view.outboundMessages.find((item) => item.id === id);
      if (record) {
        view.selectedRecords.add(mailRecordKey({ ...record, __kind: record.thread_key ? 'thread' : 'outbound' }));
        renderBulkBar(currentPageRecords(), currentRecords());
        openRouteDrawer();
      }
      return;
    }
    if (action === 'compose-campaign') {
      openComposer({ campaignId: id });
      return;
    }
    if (action === 'open-outbound') {
      location.hash = `outbound?campaign_id=${encodeURIComponent(id)}`;
      return;
    }
    if (action === 'refresh-status') {
      await runBusy(async () => {
        await dispatchOutbound('outbound.provider.reconcile', id, { message_id: id });
        showToast('Zustellstatus wurde mit dem Mailserver abgeglichen.');
      });
      return;
    }
    if (action === 'campaign-pause' || action === 'campaign-resume') {
      await runBusy(async () => {
        await dispatchOutbound('outbound.campaign.status.set', id, {
          campaign_id: id,
          status: action === 'campaign-pause' ? 'paused' : 'active',
        });
        showToast(action === 'campaign-pause' ? t('paused', 'Pausiert') : t('active', 'Aktiv'));
      });
      return;
    }
    const message = view.outboundMessages.find((item) => item.id === id);
    if (!message) return;
    await runBusy(async () => {
      if (action === 'request-approval') {
        await dispatchOutbound('outbound.message.request_approval', id, { message_id: id });
        showToast(t('approvalRequested', 'Entwurf wurde zur Freigabe eingereicht.'));
      } else if (action === 'approve') {
        await dispatchOutbound('outbound.message.approve', id, { message_id: id });
        showToast(t('approved', 'Nachricht wurde freigegeben.'));
      } else if (action === 'send') {
        await dispatchOutbound('outbound.message.send_approved', id, { message_id: id });
        showToast(t('queued', 'Nachricht wurde in die Mailserver-Queue eingereiht.'));
      }
    });
  }

  function openComposer(seed = {}) {
    refs.composer.hidden = false;
    refs.composerTitle.textContent = seed.series ? t('seriesEmail', 'Serien-E-Mail') : t('compose', 'Neue Mail');
    refs.composerKicker.textContent = seed.series ? t('campaign', 'Kampagne') : t('draft', 'Entwurf');
    refs.composeFrom.value = seed.accountKey || view.accountKey || view.accounts[0]?.account_key || '';
    refs.composeTo.value = Array.isArray(seed.to) ? seed.to.join('\n') : (seed.to || '');
    refs.composeCampaign.value = seed.campaignId || (view.scopeType === 'campaign' ? view.scopeId : '');
    refs.composeSubject.value = seed.subject || '';
    refs.composeBody.value = seed.body || '';
    setComposerStatus('');
    window.setTimeout(() => refs.composeTo.focus(), 0);
  }

  function closeComposer() {
    refs.composer.hidden = true;
    setComposerStatus('');
  }

  function populateComposerSelects() {
    if (!refs.composeFrom || !refs.composeCampaign) return;
    const currentFrom = refs.composeFrom.value;
    refs.composeFrom.innerHTML = '';
    refs.composeFrom.append(option('', t('missingSender', 'Bitte ein Absenderpostfach auswählen.')));
    for (const account of view.accounts) refs.composeFrom.append(option(account.account_key, account.address || account.account_key));
    refs.composeFrom.value = view.accounts.some((account) => account.account_key === currentFrom) ? currentFrom : (view.accountKey || view.accounts[0]?.account_key || '');

    const currentCampaign = refs.composeCampaign.value;
    refs.composeCampaign.innerHTML = '';
    refs.composeCampaign.append(option('', t('ungrouped', 'Ohne sichtbare Gruppe')));
    for (const campaign of view.campaigns.filter((item) => !item.payload?.hidden_in_mail_groups)) {
      refs.composeCampaign.append(option(campaign.id, campaign.name));
    }
    refs.composeCampaign.value = view.campaigns.some((campaign) => campaign.id === currentCampaign) ? currentCampaign : '';
  }

  async function submitComposer(mode = 'draft') {
    if (view.busy) return;
    const recipients = parseRecipientAddresses(refs.composeTo.value);
    const input = {
      accountKey: refs.composeFrom.value,
      recipient: recipients[0] || '',
      campaignId: refs.composeCampaign.value,
      subject: refs.composeSubject.value.trim(),
      body: refs.composeBody.value.trim(),
    };
    const validation = validateComposeInput(input);
    if (validation) {
      setComposerStatus(t(validation, validation), true);
      return;
    }
    await runBusy(async () => {
      setComposerStatus(t('refreshing', 'Wird aktualisiert…'));
      const campaign = await ensureComposeCampaign(input.campaignId, { ...input, recipientCount: recipients.length });
      let lastMessageId = '';
      const contentRevisionId = String(campaign.payload?.mail_group?.content?.active_revision_id || '');
      for (const recipient of recipients) {
        const ids = { engagementId: `eng_mail_${crypto.randomUUID()}`, messageId: `msg_mail_${crypto.randomUUID()}` };
        const commands = buildComposeCommandBundle({ ...input, recipient, campaignId: campaign.id, contentRevisionId }, ids, Date.now());
        await dispatchOutbound(commands.engagement.commandType, ids.engagementId, commands.engagement.payload);
        await dispatchOutbound(commands.message.commandType, ids.messageId, commands.message.payload);
        if (mode === 'approval' || mode === 'send') {
          await dispatchOutbound(commands.approval.commandType, ids.messageId, commands.approval.payload);
        }
        if (mode === 'send') {
          await dispatchOutbound('outbound.message.approve', ids.messageId, { message_id: ids.messageId });
          await dispatchOutbound('outbound.message.send_approved', ids.messageId, { message_id: ids.messageId });
        }
        lastMessageId = ids.messageId;
      }
      view.scopeType = 'campaign';
      view.scopeId = campaign.id;
      view.selectedKind = 'outbound';
      view.selectedId = lastMessageId;
      closeComposer();
      const successMessage = mode === 'send'
        ? `${recipients.length} E-Mail${recipients.length === 1 ? '' : 's'} an den Mailserver übergeben.`
        : mode === 'approval'
          ? t('approvalRequested', 'Entwurf wurde zur Freigabe eingereicht.')
          : `${recipients.length} ${t('draftsSaved', 'Entwurf/Entwürfe gespeichert.')}`;
      notify('success', successMessage);
    }, (error) => setComposerStatus(error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.'), true));
  }

  async function ensureComposeCampaign(campaignId, seed = {}) {
    const existing = view.campaigns.find((campaign) => campaign.id === campaignId);
    if (existing) return existing;
    const now = Date.now();
    const single = Number(seed.recipientCount || 1) === 1;
    const record = buildMailGroupRecord({
      id: `mail_group_${crypto.randomUUID()}`,
      templateId: single ? 'single' : 'outbound-sales',
      name: String(seed.subject || seed.recipient || '').trim() || (single ? 'Einzel-E-Mail' : 'Serien-E-Mail'),
      ownerId: String(ctx.session?.user?.id || ''),
      accountKey: String(seed.accountKey || ''),
    }, now);
    assertCanWriteCampaigns();
    await insertCollectionRecord(collections.outbound_campaigns, record);
    view.campaigns.push(record);
    return record;
  }

  function openGroupEditor() {
    refs.groupEditor.hidden = false;
    refs.groupTemplate.value = 'outbound-sales';
    refs.groupName.value = '';
    refs.groupDescription.value = mailGroupTemplate('outbound-sales').objective;
    window.setTimeout(() => refs.groupName.focus(), 0);
  }

  function applyGroupTemplateDefaults() {
    const template = mailGroupTemplate(refs.groupTemplate?.value || 'free');
    refs.groupDescription.value = template.objective;
  }

  function closeGroupEditor() {
    refs.groupEditor.hidden = true;
  }

  async function createGroup() {
    const name = refs.groupName.value.trim();
    if (!name) {
      showToast(t('groupNameRequired', 'Bitte einen Gruppennamen eingeben.'));
      refs.groupName.focus();
      return;
    }
    await runBusy(async () => {
      assertCanWriteCampaigns();
      const now = Date.now();
      const record = buildMailGroupRecord({
        id: `camp_mail_${crypto.randomUUID()}`,
        name,
        objective: refs.groupDescription.value.trim(),
        templateId: refs.groupTemplate?.value || 'free',
        ownerId: String(ctx.session?.user?.id || ''),
        accountKey: view.accountKey || '',
      }, now);
      await insertCollectionRecord(collections.outbound_campaigns, record);
      view.campaigns.unshift(record);
      view.scopeType = 'campaign';
      view.scopeId = record.id;
      view.selectedKind = '';
      view.selectedId = '';
      closeGroupEditor();
      showToast(t('groupCreated', 'Mailgruppe wurde erstellt.'));
    });
  }

  async function openGroupContentEditor(groupId) {
    const campaign = view.campaigns.find((item) => item.id === groupId);
    if (!campaign) return;
    const descriptor = emailGroupDescriptor(campaign);
    const content = campaign.payload?.mail_group?.content || {};
    const envelope = content.editor_envelope || {};
    if (view.contentEditor) await view.contentEditor.destroy();
    view.contentEditor = null;
    view.contentGroupId = campaign.id;
    refs.contentTitle.textContent = campaign.name;
    refs.contentSurface.dataset.contextRecordId = campaign.id;
    refs.contentSurface.dataset.contextRecordType = 'outbound_campaign';
    refs.contentSurface.dataset.contextLabel = campaign.name;
    refs.contentSurface.hidden = false;
    refs.saveContent.disabled = false;
    view.contentEditor = await createMailContentEditor({
      ctx,
      host: refs.contentHost,
      commandHost: refs.contentControls,
      locale: ctx.locale,
      mode: descriptor.contentMode === 'word' ? 'rich-text' : 'html',
      documentArtifact: envelope.documentArtifact || null,
      htmlDocument: envelope.htmlDocument || {},
      mergeTags: {
        vorname: 'Anna',
        nachname: 'Beispiel',
        unternehmen: 'Beispiel GmbH',
        email: 'anna@example.test',
      },
      onEvent({ name }) {
        if (name === 'change') refs.saveContent.disabled = false;
      },
    });
    refs.contentHost.querySelector('button, [tabindex]')?.focus?.({ preventScroll: true });
  }

  async function closeGroupContentEditor() {
    await view.contentEditor?.destroy?.();
    view.contentEditor = null;
    view.contentGroupId = '';
    refs.contentHost.replaceChildren();
    delete refs.contentSurface.dataset.contextRecordId;
    delete refs.contentSurface.dataset.contextRecordType;
    delete refs.contentSurface.dataset.contextLabel;
    refs.contentSurface.hidden = true;
  }

  async function saveGroupContent() {
    if (!view.contentEditor || !view.contentGroupId || view.busy) return;
    const campaign = view.campaigns.find((item) => item.id === view.contentGroupId);
    if (!campaign) return;
    await runBusy(async () => {
      assertCanWriteCampaigns();
      refs.saveContent.disabled = true;
      const envelope = await view.contentEditor.serialize();
      const revisionId = `mail_content_${crypto.randomUUID()}`;
      const sourcePayload = envelope.mode === 'rich-text'
        ? {
          document_id: envelope.documentArtifact?.documentId || '',
          document_version_id: envelope.documentArtifact?.versionId || '',
        }
        : {
          format: envelope.format,
          document: envelope.htmlDocument || {},
          mjml: String(envelope.mjml || ''),
        };
      const compiledHtml = String(envelope.compiledHtml || envelope.html || '');
      const compiledText = String(envelope.compiledText || envelope.text || '');
      const sourceSha256 = String(envelope.sourceSha256 || '') || await sha256Text(stableJson(sourcePayload));
      const compiledSha256 = compiledHtml ? await sha256Text(compiledHtml) : '';
      const revision = buildMailContentRevision({
        id: revisionId,
        groupId: campaign.id,
        editorKind: envelope.mode === 'rich-text' ? 'word' : 'email_visual',
        sourceRef: sourcePayload,
        sourceSha256,
        compiledHtmlRef: envelope.compiledHtmlRef || (compiledHtml ? { storage: 'embedded', media_type: 'text/html', content: compiledHtml } : null),
        compiledTextRef: envelope.compiledTextRef || (compiledText ? { storage: 'embedded', media_type: 'text/plain', content: compiledText } : null),
        compiledAssets: envelope.compiledAssets || [],
        diagnostics: envelope.diagnostics || [],
        compiledSha256,
        compilerId: String(envelope.compilerId || ''),
        state: compiledHtml ? 'frozen' : 'draft',
      });
      const groupPayload = campaign.payload?.mail_group || {};
      const nextContent = appendMailContentRevision(groupPayload.content || {}, revision);
      const next = {
        ...campaign,
        payload: {
          ...(campaign.payload || {}),
          mail_group: {
            ...groupPayload,
            content: {
              ...nextContent,
              mode: envelope.mode === 'rich-text' ? 'word' : 'email_visual',
              editor_envelope: envelope,
            },
          },
        },
        updated_at_ms: Date.now(),
      };
      await upsertCollectionRecord(collections.outbound_campaigns, next);
      view.campaigns = view.campaigns.map((item) => item.id === next.id ? next : item);
      const adoptedMessages = applyContentRevisionToPending(view.outboundMessages, revision);
      const changedMessages = adoptedMessages.filter((message, index) => message !== view.outboundMessages[index]);
      for (const message of changedMessages) {
        await upsertCollectionRecord(collections.outbound_messages, message);
      }
      view.outboundMessages = adoptedMessages;
      view.contentEditor.markClean();
      showToast('Gruppeninhalt gespeichert. Bereits versendete E-Mails bleiben unverändert.');
      renderNavigation();
      renderDetail();
    }, (error) => {
      refs.saveContent.disabled = false;
      showToast(error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.'));
    });
  }

  async function openMailboxAdmin() {
    if (!isGlobalMailAdmin(ctx.session?.user || {})) {
      showToast(t('mailboxAdminDenied', 'Nur Administratoren dürfen Postfächer verwalten.'));
      return;
    }
    closeComposer();
    closeGroupEditor();
    closeMobileNavigation();
    view.mailserver.open = true;
    view.mailserver.error = '';
    view.mailserver.message = '';
    refs.mailboxAdmin.hidden = false;
    renderMailboxAdmin();
    await loadMailserverConfig();
  }

  function closeMailboxAdmin() {
    view.mailserver.open = false;
    view.mailserver.pendingDelete = '';
    if (refs.mailboxPassword) refs.mailboxPassword.value = '';
    refs.mailboxAdmin.hidden = true;
  }

  function openMobileNavigation() {
    root.classList.add('is-mobile-nav-open');
  }

  function closeMobileNavigation() {
    root.classList.remove('is-mobile-nav-open');
  }

  async function loadMailserverConfig() {
    if (!view.mailserver.open || view.mailserver.loading) return;
    view.mailserver.loading = true;
    view.mailserver.error = '';
    view.mailserver.message = '';
    renderMailboxAdmin();
    try {
      const command = await dispatchCommand('ctox', 'ctox.mailserver.get_config', 'mailserver-config', {}, {
        project: false,
        refresh: false,
        renderAfter: false,
      });
      const outcome = commandOutcome(command);
      view.mailserver.domains = Array.isArray(outcome.domains) ? outcome.domains : [];
      view.mailserver.users = Array.isArray(outcome.users) ? outcome.users : [];
      view.mailserver.runtimeConfig = outcome.runtime_config || {};
      view.mailserver.runtimeStatus = outcome.runtime_status || {};
      view.mailserver.health = outcome.health || {};
    } catch (error) {
      console.error('[mail] failed to load mailserver configuration', error);
      view.mailserver.error = error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.');
    } finally {
      view.mailserver.loading = false;
      renderMailboxAdmin();
    }
  }

  async function saveMailserverRuntime(event) {
    event?.preventDefault?.();
    if (!isGlobalMailAdmin(ctx.session?.user || {}) || view.mailserver.loading) return;
    const payload = {
      enabled: view.mailserver.runtimeConfig.enabled !== false,
      hostname: refs.serverHostname.value.trim(),
      bind_host: refs.serverBindHost.value.trim(),
      smtp_port: Number(refs.serverSmtpPort.value),
      imap_port: Number(refs.serverImapPort.value),
      outbound_throttle_per_min: Number(refs.serverThrottle.value),
      max_connections: Number(refs.serverConnections.value),
      tracking_base_url: refs.serverTrackingUrl.value.trim(),
    };
    await persistMailserverRuntime(payload, 'Serverkonfiguration gespeichert und angewendet.');
  }

  async function toggleMailserverRuntime() {
    if (!isGlobalMailAdmin(ctx.session?.user || {}) || view.mailserver.loading) return;
    const enabled = view.mailserver.runtimeConfig.enabled === false;
    await persistMailserverRuntime(
      { ...view.mailserver.runtimeConfig, enabled },
      enabled ? 'Mailserver gestartet.' : 'Mailserver gestoppt.',
    );
  }

  async function persistMailserverRuntime(payload, successMessage) {
    view.mailserver.loading = true;
    view.mailserver.error = '';
    view.mailserver.message = '';
    renderMailboxAdmin();
    try {
      await dispatchCommand('ctox', 'ctox.mailserver.save_runtime', 'mailserver-runtime', payload, {
        project: false, refresh: false, renderAfter: false,
      });
      view.mailserver.loading = false;
      await loadMailserverConfig();
      view.mailserver.message = successMessage;
    } catch (error) {
      view.mailserver.error = error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.');
    } finally {
      view.mailserver.loading = false;
      renderMailboxAdmin();
    }
  }

  async function saveMailserverDomain(event) {
    event?.preventDefault?.();
    if (!isGlobalMailAdmin(ctx.session?.user || {}) || view.mailserver.loading) return;
    const domainName = refs.domainName.value.trim().toLowerCase();
    if (!domainName || !domainName.includes('.')) {
      view.mailserver.error = 'Bitte eine gültige Versanddomain eingeben.';
      renderMailboxAdmin();
      return;
    }
    view.mailserver.loading = true;
    view.mailserver.error = '';
    renderMailboxAdmin();
    try {
      await dispatchCommand('ctox', 'ctox.mailserver.save_domain', domainName, {
        domain_name: domainName,
        dkim_selector: refs.dkimSelector.value.trim() || 'default',
        spf_record: refs.spfRecord.value.trim(),
        dmarc_record: refs.dmarcRecord.value.trim(),
      }, { project: false, refresh: false, renderAfter: false });
      refs.domainName.value = '';
      refs.dmarcRecord.value = '';
      view.mailserver.loading = false;
      await loadMailserverConfig();
      view.mailserver.message = 'Versanddomain gespeichert. Die DNS-Werte stehen unten zum Kopieren bereit.';
    } catch (error) {
      view.mailserver.error = error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.');
    } finally {
      view.mailserver.loading = false;
      renderMailboxAdmin();
    }
  }

  async function handleDomainAction(action, domainName) {
    if (!domainName || view.mailserver.loading) return;
    if (action === 'request-delete') {
      view.mailserver.pendingDomainDelete = domainName;
      renderMailboxAdmin();
      return;
    }
    if (action === 'cancel-delete') {
      view.mailserver.pendingDomainDelete = '';
      renderMailboxAdmin();
      return;
    }
    if (action !== 'confirm-delete') return;
    view.mailserver.loading = true;
    renderMailboxAdmin();
    try {
      await dispatchCommand('ctox', 'ctox.mailserver.delete_domain', domainName, { domain_name: domainName }, {
        project: false, refresh: false, renderAfter: false,
      });
      view.mailserver.pendingDomainDelete = '';
      view.mailserver.loading = false;
      await loadMailserverConfig();
      view.mailserver.message = 'Versanddomain gelöscht.';
    } catch (error) {
      view.mailserver.error = error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.');
    } finally {
      view.mailserver.loading = false;
      renderMailboxAdmin();
    }
  }

  async function createMailbox(event) {
    event?.preventDefault?.();
    if (!isGlobalMailAdmin(ctx.session?.user || {}) || view.mailserver.loading) return;
    const username = refs.mailboxUsername.value.trim().toLowerCase();
    const ownerUserId = refs.mailboxOwner.value.trim();
    const password = refs.mailboxPassword.value;
    refs.mailboxPassword.value = '';
    if (!isEmailAddress(username) || !password || !ownerUserId || !view.users.some((user) => (user.id || user.user_id) === ownerUserId)) {
      view.mailserver.error = t('mailboxRequired', 'Bitte eine gültige E-Mail-Adresse und ein Passwort eingeben.');
      view.mailserver.message = '';
      renderMailboxAdmin();
      return;
    }
    view.mailserver.loading = true;
    view.mailserver.error = '';
    view.mailserver.message = '';
    renderMailboxAdmin();
    try {
      await dispatchCommand('ctox', 'ctox.mailserver.save_user', username, {
        username,
        owner_user_id: ownerUserId,
        password,
      }, { project: false, refresh: false, renderAfter: false });
      refs.mailboxUsername.value = '';
      refs.mailboxOwner.value = '';
      await refreshData();
      view.mailserver.loading = false;
      await loadMailserverConfig();
      view.mailserver.message = t('mailboxCreated', 'Postfach wurde angelegt und dem Nutzer zugeordnet.');
    } catch (error) {
      console.error('[mail] failed to create mailbox', error);
      view.mailserver.error = error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.');
    } finally {
      view.mailserver.loading = false;
      render();
    }
  }

  async function handleMailboxAction(action, username) {
    if (!isGlobalMailAdmin(ctx.session?.user || {}) || !username || view.mailserver.loading) return;
    if (action === 'request-delete') {
      view.mailserver.pendingDelete = username;
      renderMailboxAdmin();
      return;
    }
    if (action === 'cancel-delete') {
      view.mailserver.pendingDelete = '';
      renderMailboxAdmin();
      return;
    }
    if (action !== 'confirm-delete') return;
    view.mailserver.loading = true;
    view.mailserver.error = '';
    view.mailserver.message = '';
    renderMailboxAdmin();
    try {
      await dispatchCommand('ctox', 'ctox.mailserver.delete_user', username, { username }, {
        project: false,
        refresh: false,
        renderAfter: false,
      });
      view.mailserver.pendingDelete = '';
      await refreshData();
      view.mailserver.loading = false;
      await loadMailserverConfig();
      view.mailserver.message = t('mailboxDeleted', 'Postfach wurde gelöscht.');
    } catch (error) {
      console.error('[mail] failed to delete mailbox', error);
      view.mailserver.error = error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.');
    } finally {
      view.mailserver.loading = false;
      render();
    }
  }

  function renderMailboxAdmin() {
    const admin = isGlobalMailAdmin(ctx.session?.user || {});
    if (!admin) {
      refs.mailboxAdmin.hidden = true;
      return;
    }
    refs.mailboxAdmin.hidden = !view.mailserver.open;
    if (!view.mailserver.open) return;
    refs.mailboxAdmin.setAttribute('aria-busy', String(view.mailserver.loading));
    refs.createMailbox.disabled = view.mailserver.loading;
    refs.reloadMailboxes.disabled = view.mailserver.loading;

    const config = view.mailserver.runtimeConfig || {};
    const status = view.mailserver.runtimeStatus || {};
    const health = view.mailserver.health || {};
    const running = config.enabled !== false && status.state !== 'stopped';
    refs.serverState.textContent = running
      ? (health.smtp_reachable && health.imap_reachable ? 'Mailserver läuft' : 'Mailserver startet oder ist beeinträchtigt')
      : 'Mailserver gestoppt';
    refs.serverEndpoints.textContent = `SMTP ${config.bind_host || '127.0.0.1'}:${config.smtp_port || 2525} · IMAP ${config.bind_host || '127.0.0.1'}:${config.imap_port || 1143}`;
    refs.serverDot.classList.toggle('is-running', running && health.smtp_reachable && health.imap_reachable);
    refs.serverDot.classList.toggle('is-degraded', running && !(health.smtp_reachable && health.imap_reachable));
    refs.toggleServer.textContent = running ? 'Stoppen' : 'Starten';
    refs.toggleServer.disabled = view.mailserver.loading;
    refs.queueCount.textContent = String(health.queue_pending || 0);
    refs.deliveredCount.textContent = String(health.delivery_success || 0);
    refs.failedCount.textContent = String(health.delivery_failed || 0);
    refs.serverHostname.value = config.hostname || 'localhost';
    refs.serverBindHost.value = config.bind_host || '127.0.0.1';
    refs.serverSmtpPort.value = String(config.smtp_port || 2525);
    refs.serverImapPort.value = String(config.imap_port || 1143);
    refs.serverThrottle.value = String(config.outbound_throttle_per_min || 120);
    refs.serverConnections.value = String(config.max_connections || 10);
    refs.serverTrackingUrl.value = config.tracking_base_url || '';
    refs.saveServer.disabled = view.mailserver.loading;
    refs.saveDomain.disabled = view.mailserver.loading;

    const domains = view.mailserver.domains.filter((domain) => mailserverDomain(domain));
    refs.domainList.innerHTML = domains.length
      ? domains.map((domain) => renderDomainCard(domain)).join('')
      : `<span class="mail-mailbox-owner">${escapeHtml(t('noDomains', 'Keine Domain konfiguriert'))}</span>`;

    const users = view.mailserver.users.map(mailserverUsername).filter(Boolean).sort((a, b) => a.localeCompare(b));
    refs.mailboxCount.textContent = String(users.length);
    refs.mailboxList.innerHTML = users.length
      ? users.map((username) => renderMailboxRow(username)).join('')
      : `<div class="ctox-empty"><span>${escapeHtml(t('noManagedMailboxes', 'Noch keine Mailserver-Postfächer.'))}</span></div>`;

    refs.mailboxStatus.textContent = view.mailserver.loading
      ? t('loadingMailboxes', 'Mailserver-Konfiguration wird geladen…')
      : (view.mailserver.error || view.mailserver.message || '');
    refs.mailboxStatus.classList.toggle('is-error', Boolean(view.mailserver.error));
  }

  function renderDomainCard(domain) {
    const name = mailserverDomain(domain);
    const pending = view.mailserver.pendingDomainDelete === name;
    const actions = pending
      ? `<button type="button" class="ctox-button ctox-button--sm mail-mailbox-delete" data-mail-domain-action="confirm-delete" data-mail-domain-name="${escapeAttribute(name)}">Wirklich löschen</button><button type="button" class="ctox-button ctox-button--sm" data-mail-domain-action="cancel-delete" data-mail-domain-name="${escapeAttribute(name)}">Abbrechen</button>`
      : `<button type="button" class="ctox-button ctox-button--sm" data-mail-domain-action="request-delete" data-mail-domain-name="${escapeAttribute(name)}">Entfernen</button>`;
    const selector = String(domain.dkim_selector || 'default');
    const publicKey = String(domain.dkim_public_key || '').replace(/-----[^-]+-----|\s+/g, '');
    const dkim = `v=DKIM1; k=rsa; p=${publicKey}`;
    return `<article class="mail-domain-card"><header><div><strong>${escapeHtml(name)}</strong><span>DNS-Konfiguration</span></div><div>${actions}</div></header><dl><div><dt>${escapeHtml(selector)}._domainkey</dt><dd>${escapeHtml(dkim)}</dd></div><div><dt>@ · SPF</dt><dd>${escapeHtml(domain.spf_record || '')}</dd></div><div><dt>_dmarc</dt><dd>${escapeHtml(domain.dmarc_record || '')}</dd></div></dl></article>`;
  }

  function renderMailboxRow(username) {
    const account = view.accounts.find((candidate) => (
      String(candidate.address || '').toLowerCase() === username.toLowerCase()
      || String(candidate.account_key || '').toLowerCase() === `email:${username.toLowerCase()}`
    ));
    const owner = String(accountProfile(account).owner_user_id || accountProfile(account).ownerUserId || '').trim();
    const ownerUser = view.users.find((user) => String(user.id || user.user_id) === owner);
    const ownerLabel = owner
      ? `${t('assignedTo', 'Zugeordnet zu')}: ${ownerUser ? userDisplayName(ownerUser) : owner}`
      : t('unassigned', 'Nicht zugeordnet');
    const pending = view.mailserver.pendingDelete === username;
    const actions = pending
      ? `<button type="button" class="ctox-button ctox-button--sm mail-mailbox-delete" data-mailbox-action="confirm-delete" data-mailbox-username="${escapeAttribute(username)}">${escapeHtml(t('confirmDelete', 'Wirklich löschen'))}</button>
         <button type="button" class="ctox-button ctox-button--sm" data-mailbox-action="cancel-delete" data-mailbox-username="${escapeAttribute(username)}">${escapeHtml(t('cancel', 'Abbrechen'))}</button>`
      : `<button type="button" class="ctox-button ctox-button--sm mail-mailbox-delete" data-mailbox-action="request-delete" data-mailbox-username="${escapeAttribute(username)}">${escapeHtml(t('delete', 'Löschen'))}</button>`;
    return `<div class="mail-mailbox-row">
      <div class="mail-mailbox-identity"><div class="mail-mailbox-address">${escapeHtml(username)}</div><div class="mail-mailbox-owner">${escapeHtml(ownerLabel)}</div></div>
      <div class="mail-mailbox-actions">${actions}</div>
    </div>`;
  }

  async function dispatchOutbound(commandType, recordId, payload, options = {}) {
    return dispatchCommand('outbound', commandType, recordId, payload, options);
  }

  async function dispatchCommand(module, commandType, recordId, payload, options = {}) {
    if (!ctx.commandBus?.dispatch) throw new Error('CTOX command bus is required for mail actions');
    const commandId = `cmd_mail_${crypto.randomUUID()}`;
    const result = await ctx.commandBus.dispatch({
      id: commandId,
      command_id: commandId,
      module,
      command_type: commandType,
      record_id: recordId,
      inbound_channel: 'business_os.mail',
      status: 'pending_sync',
      payload,
      client_context: { source_module: 'mail' },
    });
    const command = await waitForCommand(result?.command_id || result?.id || commandId, result);
    if (command?.status === 'failed' || command?.status === 'blocked' || command?.status === 'cancelled') {
      throw new Error(command.error || command.result?.error || `${commandType} failed`);
    }
    if (options.project !== false) await projectCommandResult(command?.result || result?.result || {});
    if (options.refresh !== false) await refreshData();
    if (options.renderAfter !== false) render();
    return command;
  }

  async function waitForCommand(commandId, initial, timeoutMs = 45000) {
    if (TERMINAL_COMMAND_STATUSES.has(initial?.status)) return initial;
    if (!collections.business_commands) return initial;
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const document = await collections.business_commands.findOne(commandId).exec();
      const command = document?.toJSON?.() || document || null;
      if (command && TERMINAL_COMMAND_STATUSES.has(command.status)) return command;
      await sleep(250);
    }
    throw new Error(`Mail command ${commandId} was not acknowledged by the local service.`);
  }

  async function projectCommandResult(result) {
    const projections = [
      ['outbound_engagements', result.engagement],
      ['outbound_messages', result.message],
      ['outbound_approvals', result.approval],
    ];
    for (const [name, record] of projections) {
      if (!record?.id || !collections[name]) continue;
      await upsertCollectionRecord(collections[name], record);
    }
  }

  async function runBusy(operation, onError) {
    if (view.busy) return;
    view.busy = true;
    root.classList.add('is-busy');
    try {
      await operation();
    } catch (error) {
      console.error('[mail] action failed', error);
      if (onError) onError(error);
      else showToast(error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.'));
    } finally {
      view.busy = false;
      root.classList.remove('is-busy');
    }
  }

  function setComposerStatus(message, error = false) {
    refs.composerStatus.textContent = message || '';
    refs.composerStatus.classList.toggle('is-error', Boolean(error));
  }

  function showToast(message) {
    notify('info', message);
  }

  function notify(type, message) {
    ctx.notifications?.show?.({ type, title: t('mail', 'Mail'), message: String(message || '') });
  }

  function filteredAccountThreads() {
    const allowedAccounts = new Set(view.accounts.map((account) => account.account_key));
    return view.threads.filter((thread) => (
      allowedAccounts.has(thread.account_key)
      && (!view.accountKey || thread.account_key === view.accountKey)
    ));
  }

  function filteredAccountOutboundMessages() {
    const allowedAccounts = new Set(view.accounts.map((account) => account.account_key));
    return view.outboundMessages.filter((message) => {
      const accountKey = message.communication_account_key || message.sender_account_id;
      if (!allowedAccounts.has(accountKey)) return false;
      return !view.accountKey || accountKey === view.accountKey;
    });
  }

  function assertCanWriteCampaigns() {
    if (ctx.permissions?.canWriteCollection
      && ctx.permissions.canWriteCollection('outbound_campaigns') !== true) {
      throw new Error(t('writeDenied', 'Du darfst keine Mailgruppen anlegen.'));
    }
  }

  function scopeRecords() {
    const inbound = filteredAccountThreads().map((thread) => ({ ...thread, __kind: 'thread' }));
    const outbound = filteredAccountOutboundMessages().map((message) => ({ ...message, __kind: 'outbound' }));
    let records;
    if (view.scopeType === 'campaign') {
      records = outbound.filter((message) => message.campaign_id === view.scopeId);
    } else if (view.scopeId === 'all') {
      records = [...inbound, ...outbound];
    } else if (view.scopeId === 'outbound') {
      records = outbound;
    } else if (view.scopeId === 'approval') {
      records = outbound.filter((message) => String(message.approval_status || '') === 'awaiting_approval');
    } else if (view.scopeId === 'failed') {
      records = outbound.filter((message) => String(message.send_status || '').toLowerCase().includes('fail'));
    } else if (view.scopeId === 'routed') {
      records = [...inbound, ...outbound].filter((record) => routeCommandForRecord(record, view.commands));
    } else {
      records = inbound;
    }
    return records;
  }

  function currentRecords() {
    const grammar = view.listGrammar;
    let records = scopeRecords();
    if (grammar.band !== 'all') records = records.filter((record) => recordMatchesBand(record, grammar.band, view.communicationMessages, view.commands));
    const status = grammar.filters.status || 'all';
    if (status !== 'all') records = records.filter((record) => recordMatchesStatus(record, status, view.commands));
    if (grammar.search) records = records.filter((record) => JSON.stringify(record).toLowerCase().includes(grammar.search));
    const sort = grammar.filters.sort || 'recent';
    return [...records].sort((a, b) => {
      if (sort === 'subject') return recordSubject(a).localeCompare(recordSubject(b));
      const delta = recordTime(a) - recordTime(b);
      return sort === 'oldest' ? delta : -delta;
    });
  }

  function currentScopeLabel() {
    if (view.scopeType === 'campaign') return view.campaigns.find((campaign) => campaign.id === view.scopeId)?.name || t('campaign', 'Kampagne');
    return ({
      all: t('allTraffic', 'Gesamter Mail-Verkehr'),
      inbound: t('inbox', 'Massen-Eingang'),
      outbound: t('outboundQueue', 'Massen-Ausgang'),
      approval: t('awaiting', 'Freigabe-Queue'),
      failed: t('failed', 'Fehler-Queue'),
      routed: t('routed', 'Geroutete E-Mails'),
    })[view.scopeId] || t('inbox', 'Massen-Eingang');
  }

  function currentPageRecords() {
    const records = currentRecords();
    return records.slice(view.page * view.pageSize, (view.page + 1) * view.pageSize);
  }

  function changePage(delta) {
    const maxPage = Math.max(0, Math.ceil(currentRecords().length / view.pageSize) - 1);
    view.page = clamp(view.page + delta, 0, maxPage);
    renderList();
    refs.recordList?.querySelector('[data-mail-record-kind]')?.focus?.();
  }

  function allSelectableRecords() {
    return [
      ...filteredAccountThreads().map((record) => ({ ...record, __kind: 'thread' })),
      ...filteredAccountOutboundMessages().map((record) => ({ ...record, __kind: 'outbound' })),
    ];
  }

  function selectedMailRecords() {
    return allSelectableRecords().filter((record) => view.selectedRecords.has(mailRecordKey(record)));
  }

  function renderBulkBar(pageRows = currentPageRecords(), allRows = currentRecords()) {
    const selected = selectedMailRecords();
    refs.bulkbar.hidden = selected.length === 0;
    refs.selectedCount.textContent = `${selected.length} ${t('selected', 'ausgewählt')}`;
    const pageKeys = pageRows.map(mailRecordKey);
    const selectedOnPage = pageKeys.filter((key) => view.selectedRecords.has(key)).length;
    refs.selectPage.checked = pageKeys.length > 0 && selectedOnPage === pageKeys.length;
    refs.selectPage.indeterminate = selectedOnPage > 0 && selectedOnPage < pageKeys.length;
    refs.selectFiltered.hidden = allRows.length === 0 || allRows.every((record) => view.selectedRecords.has(mailRecordKey(record)));
    refs.selectFiltered.textContent = `${t('selectAllFiltered', 'Alle gefilterten auswählen')} (${allRows.length})`;
    refs.bulkApproval.disabled = !selected.some((record) => record.__kind === 'outbound' && canRequestApproval(record));
    refs.bulkSend.disabled = !selected.some((record) => record.__kind === 'outbound' && String(record.approval_status || '') === 'approved');
    refs.bulkRoute.disabled = selected.length === 0;
    refs.bulkExport.disabled = selected.length === 0;
    refs.bulkbar.dataset.filteredCount = String(allRows.length);
  }

  function toggleRecordSelection(key) {
    if (!key) return;
    if (view.selectedRecords.has(key)) view.selectedRecords.delete(key);
    else view.selectedRecords.add(key);
    renderListSelection();
    renderBulkBar();
  }

  function togglePageSelection() {
    const pageRows = currentPageRecords();
    const shouldSelect = refs.selectPage.checked;
    pageRows.forEach((record) => {
      const key = mailRecordKey(record);
      if (shouldSelect) view.selectedRecords.add(key);
      else view.selectedRecords.delete(key);
    });
    renderListSelection();
    renderBulkBar(pageRows, currentRecords());
  }

  function selectAllFiltered() {
    currentRecords().forEach((record) => view.selectedRecords.add(mailRecordKey(record)));
    renderListSelection();
    renderBulkBar();
  }

  function clearSelection(renderAfter = true) {
    view.selectedRecords.clear();
    if (renderAfter) {
      renderListSelection();
      renderBulkBar();
    }
  }

  function exportSelectedMail() {
    const records = selectedMailRecords();
    if (!records.length) return;
    downloadJson({ schema: 'ctox.mail.bulk-export.v1', exported_at: new Date().toISOString(), records }, `ctox-mail-auswahl-${records.length}.json`, ctx.host);
    notify('success', `${records.length} ${t('recordsExported', 'Datensätze exportiert.')}`);
  }

  async function runBulkOutboundAction(action) {
    const records = selectedMailRecords().filter((record) => record.__kind === 'outbound');
    const eligible = records.filter((record) => action === 'request-approval'
      ? canRequestApproval(record)
      : String(record.approval_status || '') === 'approved');
    if (!eligible.length) return;
    await runBusy(async () => {
      let completed = 0;
      for (const record of eligible) {
        const commandType = action === 'request-approval' ? 'outbound.message.request_approval' : 'outbound.message.send_approved';
        await dispatchOutbound(commandType, record.id, { message_id: record.id }, { refresh: false, renderAfter: false });
        completed += 1;
      }
      await refreshData();
      clearSelection(false);
      render();
      notify('success', `${completed} ${action === 'request-approval' ? t('submittedForApproval', 'Mails zur Freigabe eingereicht.') : t('queuedForSending', 'Mails zum Versand eingereiht.')}`);
    });
  }

  function openRouteDrawer() {
    const records = selectedMailRecords();
    if (!records.length) return;
    closeComposer();
    closeGroupEditor();
    closeMailboxAdmin();
    view.route.open = true;
    view.route.error = '';
    view.route.status = '';
    refs.routeDrawer.hidden = false;
    revealInspector('route');
    const destinations = view.routeDestinations.length ? view.routeDestinations : defaultMailRouteDestinations();
    refs.routeDestination.innerHTML = `<option value="">— ${escapeHtml(t('selectDestination', 'Ziel-App wählen'))} —</option>${destinations.map((destination) => `<option value="${escapeAttribute(destination.id)}">${escapeHtml(destination.title)}</option>`).join('')}`;
    refs.routeSelection.textContent = `${records.length} ${t('messages', 'Nachrichten')} · ${records.filter((record) => record.__kind === 'thread').length} Inbound · ${records.filter((record) => record.__kind === 'outbound').length} Outbound`;
    refs.routeNote.value = '';
    refs.routeStatus.textContent = '';
    window.setTimeout(() => refs.routeDestination.focus(), 0);
  }

  function closeRouteDrawer() {
    if (view.route.busy) return;
    view.route.open = false;
    refs.routeDrawer.hidden = true;
    if (view.selectedKind && view.selectedId) revealInspector('detail');
    else concealInspector();
  }

  async function confirmRoute() {
    if (view.route.busy) return;
    const destinationModule = refs.routeDestination.value;
    const records = selectedMailRecords();
    if (!destinationModule || !records.length) {
      refs.routeStatus.textContent = t('routeDestinationRequired', 'Bitte eine Ziel-App wählen.');
      refs.routeStatus.classList.add('is-error');
      return;
    }
    view.route.busy = true;
    refs.confirmRoute.disabled = true;
    refs.routeStatus.classList.remove('is-error');
    refs.routeStatus.textContent = t('routingInProgress', 'Routing wird angelegt…');
    const batchId = `mail_route_${crypto.randomUUID()}`;
    try {
      const commands = buildMailRouteCommands({
        batchId,
        destinationModule,
        mode: refs.routeMode.value,
        note: refs.routeNote.value.trim(),
        records,
        actor: ctx.session?.user || {},
      });
      const commandIds = [];
      for (let offset = 0; offset < commands.length; offset += 8) {
        const commandBatch = commands.slice(offset, offset + 8);
        const results = await Promise.all(commandBatch.map((command) => ctx.commandBus.dispatch(command)));
        results.forEach((result, index) => {
          const command = commandBatch[index];
          const commandId = result?.command_id || result?.id || command.id;
          commandIds.push(commandId);
          view.commands.unshift({ ...command, ...result, id: commandId, command_id: commandId, status: result?.status || 'pending_sync' });
        });
      }
      view.route.lastCommandId = commandIds[0] || '';
      view.route.status = `${records.length} ${t('routedTo', 'E-Mails geroutet an')} ${routeDestinationTitle(destinationModule, view.routeDestinations)} · ${batchId}`;
      refs.routeStatus.textContent = view.route.status;
      clearSelection(false);
      view.scopeType = 'queue';
      view.scopeId = 'routed';
      renderNavigation();
      renderList();
      notify('success', view.route.status);
    } catch (error) {
      view.route.error = error?.message || String(error);
      refs.routeStatus.textContent = view.route.error;
      refs.routeStatus.classList.add('is-error');
    } finally {
      view.route.busy = false;
      refs.confirmRoute.disabled = false;
    }
  }

  async function openMailImporter() {
    closeMobileNavigation();
    await openUniversalImporter(ctx, {
      side: 'left',
      moduleId: 'mail',
      entityType: 'series_email_recipients',
      commandType: 'mail.series_email.import',
      title: t('importSeriesEmail', 'Empfänger für Serien-E-Mail importieren'),
      kicker: t('mailCampaign', 'Mail-Kampagne'),
      defaultSource: 'excel',
      showFileExplorer: false,
      defaultTitle: `${t('seriesEmail', 'Serien-E-Mail')} ${new Date().toLocaleDateString(ctx.locale === 'en' ? 'en-US' : 'de-DE')}`,
      helperText: t('importHelper', 'CSV, Excel oder Text mit E-Mail-Adressen übernehmen. Die Empfänger werden vor dem Speichern noch einmal im Mail-Entwurf angezeigt.'),
      submitLabel: t('transferToMail', 'In Serien-E-Mail übernehmen'),
      submittingLabel: t('importing', 'Empfänger werden geprüft…'),
      doneLabel: t('importDone', 'Empfänger übernommen.'),
      closeOnSubmit: true,
      dispatch: false,
      definition: { target: 'mail_series_email', accepted_columns: ['email', 'e-mail', 'mail', 'recipient', 'empfaenger'] },
      clientContext: { campaign_id: view.scopeType === 'campaign' ? view.scopeId : '' },
      onImport: async ({ payload }) => {
        const recipients = extractImportRecipients(payload);
        if (!recipients.length) throw new Error(t('missingRecipient', 'Keine gültige Empfängeradresse gefunden.'));
        const campaignId = view.scopeType === 'campaign' ? view.scopeId : '';
        openComposer({ series: true, campaignId, to: recipients });
        notify('success', `${recipients.length} ${t('recipientsTransferred', 'Empfänger übernommen.')}`);
        return { status: 'prepared', recipients: recipients.length };
      },
    });
  }

  function exportVisibleMail() {
    const records = currentRecords().map(({ __kind, ...record }) => ({ kind: __kind, ...record }));
    downloadJson({
      schema: 'ctox.mail.export.v1',
      exported_at: new Date().toISOString(),
      scope: { type: view.scopeType, id: view.scopeId, account_key: view.accountKey || null },
      records,
    }, `ctox-mail-${new Date().toISOString().slice(0, 10)}.json`, ctx.host);
    notify('success', `${records.length} ${t('recordsExported', 'Datensätze exportiert.')}`);
  }

  function closeDetail() {
    view.selectedKind = '';
    view.selectedId = '';
    concealInspector();
    renderListSelection();
  }

  function revealInspector(mode = 'detail') {
    root.classList.remove('is-inspector-hidden');
    root.classList.add('is-inspector-open', 'is-detail-open');
    root.classList.toggle('is-route-open', mode === 'route');
  }

  function concealInspector() {
    root.classList.remove('is-inspector-open', 'is-route-open', 'is-detail-open');
    root.classList.add('is-inspector-hidden');
  }

  function accountLabel(accountKey) {
    const account = view.accounts.find((item) => item.account_key === accountKey);
    return account?.address || accountKey;
  }

  function applyDeepLink() {
    const raw = String(location.hash || '');
    const query = raw.includes('?') ? raw.slice(raw.indexOf('?') + 1) : '';
    const params = new URLSearchParams(query);
    const campaignId = params.get('campaign_id') || '';
    const action = params.get('action') || '';
    const recipients = parseRecipientAddresses(params.get('recipients') || '');
    const messageId = params.get('message_id') || '';
    const threadKey = params.get('thread_key') || '';
    if (campaignId && view.campaigns.some((campaign) => campaign.id === campaignId)) {
      view.scopeType = 'campaign';
      view.scopeId = campaignId;
    }
    if (action === 'series-email') {
      view.pendingSeriesHandoff = {
        series: true,
        campaignId,
        to: recipients,
        subject: params.get('subject') || '',
        body: params.get('body') || '',
      };
    }
    if (messageId) {
      view.selectedKind = 'outbound';
      view.selectedId = messageId;
    } else if (threadKey) {
      view.selectedKind = 'thread';
      view.selectedId = threadKey;
    }
  }
}

function collectRefs(root) {
  const one = (selector) => root.querySelector(selector);
  return {
    leftPane: one('[data-mail-left-pane]'),
    navigationTitle: one('[data-mail-navigation-title]'),
    listPane: one('[data-mail-list-pane]'),
    account: one('[data-mail-account]'),
    search: one('[data-mail-search]'),
    compose: one('[data-mail-compose]'),
    composerTitle: one('[data-mail-composer-title]'),
    composerKicker: one('[data-mail-composer-kicker]'),
    importButtons: [...root.querySelectorAll('[data-mail-import], [data-mail-list-import]')],
    exportButtons: [...root.querySelectorAll('[data-mail-export], [data-mail-list-export]')],
    settings: one('[data-mail-settings]'),
    scopeList: one('[data-mail-scope-list]'),
    groupList: one('[data-mail-group-list]'),
    newGroup: one('[data-mail-new-group]'),
    sidebarFooter: one('[data-mail-sidebar-footer]'),
    listKicker: one('[data-mail-list-kicker]'),
    listTitle: one('[data-mail-list-title]'),
    recordList: one('[data-mail-record-list]'),
    listEmpty: one('[data-mail-list-empty]'),
    emptyTitle: one('[data-mail-empty-title]'),
    emptyBody: one('[data-mail-empty-body]'),
    listFooter: one('[data-mail-list-footer]'),
    prevPage: one('[data-mail-prev-page]'),
    nextPage: one('[data-mail-next-page]'),
    bulkbar: one('[data-mail-bulkbar]'),
    selectPage: one('[data-mail-select-page]'),
    selectFiltered: one('[data-mail-select-filtered]'),
    selectedCount: one('[data-mail-selected-count]'),
    bulkRoute: one('[data-mail-bulk-route]'),
    bulkApproval: one('[data-mail-bulk-approval]'),
    bulkSend: one('[data-mail-bulk-send]'),
    bulkExport: one('[data-mail-bulk-export]'),
    clearSelection: one('[data-mail-clear-selection]'),
    openNav: one('[data-mail-open-nav]'),
    closeNav: one('[data-mail-close-nav]'),
    closeDetail: one('[data-mail-close-detail]'),
    detailTitle: one('[data-mail-detail-title]'),
    detail: one('[data-mail-detail]'),
    contentSurface: one('[data-mail-content-surface]'),
    contentTitle: one('[data-mail-content-title]'),
    contentHost: one('[data-mail-content-host]'),
    contentControls: one('[data-mail-content-controls]'),
    closeContentEditor: one('[data-mail-close-content-editor]'),
    saveContent: one('[data-mail-save-content]'),
    composer: one('[data-mail-composer]'),
    closeComposer: one('[data-mail-close-composer]'),
    composeFrom: one('[data-mail-compose-from]'),
    composeTo: one('[data-mail-compose-to]'),
    composeCampaign: one('[data-mail-compose-campaign]'),
    composeSubject: one('[data-mail-compose-subject]'),
    composeBody: one('[data-mail-compose-body]'),
    composerStatus: one('[data-mail-composer-status]'),
    saveDraft: one('[data-mail-save-draft]'),
    requestApproval: one('[data-mail-request-approval]'),
    sendNow: one('[data-mail-send-now]'),
    groupEditor: one('[data-mail-group-editor]'),
    closeGroup: one('[data-mail-close-group]'),
    groupName: one('[data-mail-group-name]'),
    groupTemplate: one('[data-mail-group-template]'),
    groupDescription: one('[data-mail-group-description]'),
    createGroup: one('[data-mail-create-group]'),
    mailboxAdmin: one('[data-mail-mailbox-admin]'),
    closeMailboxAdmin: one('[data-mail-close-mailbox-admin]'),
    reloadMailboxes: one('[data-mail-reload-mailboxes]'),
    serverForm: one('[data-mail-server-form]'),
    serverState: one('[data-mail-server-state]'),
    serverEndpoints: one('[data-mail-server-endpoints]'),
    serverDot: one('[data-mail-server-dot]'),
    toggleServer: one('[data-mail-toggle-server]'),
    queueCount: one('[data-mail-queue-count]'),
    deliveredCount: one('[data-mail-delivered-count]'),
    failedCount: one('[data-mail-failed-count]'),
    serverHostname: one('[data-mail-server-hostname]'),
    serverBindHost: one('[data-mail-server-bind-host]'),
    serverSmtpPort: one('[data-mail-server-smtp-port]'),
    serverImapPort: one('[data-mail-server-imap-port]'),
    serverThrottle: one('[data-mail-server-throttle]'),
    serverConnections: one('[data-mail-server-connections]'),
    serverTrackingUrl: one('[data-mail-server-tracking-url]'),
    saveServer: one('[data-mail-save-server]'),
    domainForm: one('[data-mail-domain-form]'),
    domainName: one('[data-mail-domain-name]'),
    dkimSelector: one('[data-mail-dkim-selector]'),
    spfRecord: one('[data-mail-spf-record]'),
    dmarcRecord: one('[data-mail-dmarc-record]'),
    saveDomain: one('[data-mail-save-domain]'),
    mailboxForm: one('[data-mail-mailbox-form]'),
    mailboxUsername: one('[data-mail-mailbox-username]'),
    mailboxOwner: one('[data-mail-mailbox-owner]'),
    mailboxPassword: one('[data-mail-mailbox-password]'),
    createMailbox: one('[data-mail-create-mailbox]'),
    domainList: one('[data-mail-domain-list]'),
    mailboxStatus: one('[data-mail-mailbox-status]'),
    mailboxCount: one('[data-mail-mailbox-count]'),
    mailboxList: one('[data-mail-mailbox-list]'),
    routeDrawer: one('[data-mail-route-drawer]'),
    closeRoute: one('[data-mail-close-route]'),
    cancelRoute: one('[data-mail-cancel-route]'),
    confirmRoute: one('[data-mail-confirm-route]'),
    routeSelection: one('[data-mail-route-selection]'),
    routeDestination: one('[data-mail-route-destination]'),
    routeMode: one('[data-mail-route-mode]'),
    routeNote: one('[data-mail-route-note]'),
    routeStatus: one('[data-mail-route-status]'),
  };
}

function renderActionIcons(ctx, refs) {
  const icon = (name) => mailActionIcon(ctx, name);
  const assignments = [
    [refs.compose, 'edit'], [refs.newGroup, 'add'], [refs.settings, 'settings'], [refs.openNav, 'columns'],
    [refs.closeNav, 'close'], [refs.closeDetail, 'chevronLeft'],
    [refs.closeComposer, 'close'], [refs.closeGroup, 'close'], [refs.closeMailboxAdmin, 'close'],
    [refs.closeContentEditor, 'chevronLeft'], [refs.saveContent, 'check'],
    [refs.clearSelection, 'close'], [refs.closeRoute, 'close'],
    [refs.prevPage, 'chevronLeft'], [refs.nextPage, 'chevronRight'],
  ];
  refs.importButtons.forEach((button) => assignments.push([button, 'download']));
  refs.exportButtons.forEach((button) => assignments.push([button, 'export']));
  refs.leftPane?.querySelectorAll('[data-pg-tray-toggle]').forEach((button) => assignments.push([button, 'filter']));
  refs.listPane?.querySelectorAll('[data-pg-tray-toggle]').forEach((button) => assignments.push([button, 'filter']));
  refs.leftPane?.querySelectorAll('[data-pg-reset]').forEach((button) => assignments.push([button, 'refresh']));
  refs.listPane?.querySelectorAll('[data-pg-reset]').forEach((button) => assignments.push([button, 'refresh']));
  assignments.forEach(([button, name]) => { if (button) button.innerHTML = icon(name); });
}

// The shell normally supplies these glyphs. Keep a local stroke fallback so
// standalone fixtures and a briefly unavailable shell never render blank
// controls (especially close/navigation actions and delivery progress).
const MAIL_ICON_FALLBACK_PATHS = Object.freeze({
  add: 'M12 5v14M5 12h14',
  check: 'M4.5 12.5l5 5L19.5 7',
  chevronLeft: 'M15 6l-6 6 6 6',
  chevronRight: 'M9 6l6 6-6 6',
  clock: 'M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15ZM12 8v4.5l3 2',
  close: 'M6 6l12 12M18 6L6 18',
  columns: 'M4 4h16v16H4zM10 4v16M16 4v16',
  download: 'M12 4v11M12 15l-4-4M12 15l4-4M5 19h14',
  edit: 'M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17v3ZM14.5 6.5l3 3',
  export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  eye: 'M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6ZM12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z',
  filter: 'M4 6h16M7 12h10M10 18h4',
  grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
  link: 'M10 14a4 4 0 0 0 6 .4l3-3a4 4 0 0 0-5.6-5.6L12 7.2M14 10a4 4 0 0 0-6-.4l-3 3a4 4 0 0 0 5.6 5.6L12 16.8',
  list: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
  more: 'M6 12h.01M12 12h.01M18 12h.01',
  refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4',
  send: 'M4 12 20 4l-4 16-4.5-6.5L4 12ZM11.5 13.5 20 4',
  settings: 'M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2 5.6 5.6',
  warning: 'M12 4 2.8 19.5h18.4L12 4ZM12 10v4M12 17h.01',
});

function mailActionIcon(ctx, name, size = 16, strokeWidth = 1.8) {
  const fromShell = ctx?.getActionIcon?.(name, size, strokeWidth);
  if (typeof fromShell === 'string' && fromShell.trim()) return fromShell;
  const path = MAIL_ICON_FALLBACK_PATHS[name] || MAIL_ICON_FALLBACK_PATHS.more;
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="mail-action-icon mail-action-${name}"><path d="${path}"></path></svg>`;
}

// One-button view switch (operator directive, 31.08.2026). The button is an
// action, not a state, so it carries no aria-pressed; icon, aria-label and
// title always describe the view it switches TO. `data-pg-view` stays on it
// because shared/pane-grammar.js reads a pane's view from there - it must
// always hold the CURRENT view, otherwise an unrelated grammar emit (a search
// keystroke, a tray filter, a band tab) would flip the mode as a side effect.
function viewToggleLabel(currentView, t) {
  return currentView === 'list'
    ? t('showAsCards', 'Als Karten anzeigen')
    : t('showAsList', 'Als Liste anzeigen');
}

function syncViewToggleButton(ctx, pane, currentView, t) {
  const button = pane?.querySelector?.('[data-mail-view-toggle]');
  if (!button) return;
  const cards = currentView !== 'list';
  const label = viewToggleLabel(currentView, t);
  button.setAttribute('data-pg-view', cards ? 'cards' : 'list');
  // The shell's generic view-button wiring stamps aria-pressed on click; a
  // single toggle is an action, so the state attribute is removed again.
  button.removeAttribute('aria-pressed');
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.innerHTML = mailActionIcon(ctx, cards ? 'list' : 'grid');
}

function normalizePaneGrammar(detail, fallback) {
  return {
    search: String(detail?.search ?? fallback.search ?? '').trim().toLowerCase(),
    view: detail?.view || fallback.view || 'cards',
    band: detail?.band || fallback.band || '',
    filters: { ...(fallback.filters || {}), ...(detail?.filters || {}) },
  };
}

function writePaneCounts(pane, counts) {
  pane?.__ctoxPaneGrammar?.setCounts?.(counts);
  for (const [key, value] of Object.entries(counts || {})) {
    const node = pane?.querySelector?.(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${Number(value || 0)})`;
  }
}

function userDisplayName(user) {
  return String(user?.display_name || user?.name || user?.email || user?.login || user?.id || user?.user_id || '—');
}

function mailRecordKey(record) {
  const kind = record?.__kind || (record?.thread_key ? 'thread' : 'outbound');
  return `${kind}:${String(record?.thread_key || record?.id || '')}`;
}

function mailRecordId(record) {
  return String(record?.thread_key || record?.id || '');
}

function routeCommandForRecord(record, commands = []) {
  const recordId = mailRecordId(record);
  if (!recordId) return null;
  return [...(commands || [])]
    .filter((command) => {
      const payload = command?.payload || {};
      const sourceIds = arrayStrings(payload.source_record_ids || payload.sourceRecordIds);
      const isRoute = payload.route_kind === 'mail.bulk.route'
        || command?.client_context?.surface === 'mail.bulk.route';
      return isRoute && (sourceIds.includes(recordId) || String(command?.record_id || '') === recordId);
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0] || null;
}

function routeTargetLabel(command) {
  return String(command?.payload?.target_module || command?.client_context?.target_module || command?.module || 'App');
}

function defaultMailRouteDestinations() {
  return [
    { id: 'support', title: 'Support' },
    { id: 'tickets', title: 'Tickets' },
    { id: 'customers', title: 'Kunden' },
    { id: 'matching', title: 'Matching' },
    { id: 'outbound', title: 'Outbound' },
    { id: 'documents', title: 'Dokumente' },
    { id: 'reports', title: 'Reports' },
  ];
}

function routeDestinationsFromCatalog(catalogs = [], permissions) {
  const seen = new Map(defaultMailRouteDestinations().map((item) => [item.id, item]));
  for (const catalog of catalogs || []) {
    for (const module of Array.isArray(catalog?.modules) ? catalog.modules : []) {
      const id = String(module?.id || module?.module_id || '').trim();
      if (!id || id === 'mail' || module?.hidden === true || module?.installed === false) continue;
      if (typeof permissions?.canViewModule === 'function' && permissions.canViewModule(id) === false) continue;
      seen.set(id, { id, title: String(module?.title || module?.name || id) });
    }
  }
  return [...seen.values()].sort((a, b) => {
    if (a.id === 'support') return -1;
    if (b.id === 'support') return 1;
    return a.title.localeCompare(b.title);
  });
}

function routeDestinationTitle(id, destinations = []) {
  return destinations.find((item) => item.id === id)?.title
    || defaultMailRouteDestinations().find((item) => item.id === id)?.title
    || id;
}

function mailQueueDefinitions({ threads = [], outboundMessages = [], communicationMessages = [], commands = [], t = (_key, fallback) => fallback } = {}) {
  const allRecords = [
    ...threads.map((record) => ({ ...record, __kind: 'thread' })),
    ...outboundMessages.map((record) => ({ ...record, __kind: 'outbound' })),
  ];
  const latest = Math.max(0, ...allRecords.map(recordTime));
  const routed = allRecords.filter((record) => routeCommandForRecord(record, commands));
  return [
    { id: 'all', title: t('allTraffic', 'Gesamter Mail-Verkehr'), meta: t('inboundOutbound', 'Eingang und Ausgang'), count: allRecords.length, updatedAt: latest },
    { id: 'inbound', title: t('inbox', 'Massen-Eingang'), meta: `${threads.filter((item) => Number(item.unread_count || 0) > 0).length} ${t('unread', 'ungelesen')}`, count: threads.length, updatedAt: Math.max(0, ...threads.map(recordTime)) },
    { id: 'outbound', title: t('outboundQueue', 'Massen-Ausgang'), meta: `${outboundMessages.filter((item) => String(item.approval_status || '').toLowerCase() === 'approved').length} ${t('approvedCount', 'freigegeben')}`, count: outboundMessages.length, updatedAt: Math.max(0, ...outboundMessages.map(recordTime)) },
    { id: 'approval', title: t('awaiting', 'Freigabe-Queue'), meta: t('governedSending', 'Governed Sending'), count: outboundMessages.filter((item) => String(item.approval_status || '') === 'awaiting_approval').length, updatedAt: latest },
    { id: 'failed', title: t('failed', 'Fehler-Queue'), meta: t('retryRequired', 'Prüfung erforderlich'), count: outboundMessages.filter((item) => String(item.send_status || '').toLowerCase().includes('fail')).length, updatedAt: latest },
    { id: 'routed', title: t('routed', 'Geroutete E-Mails'), meta: t('crossAppHandoffs', 'App-Übergaben'), count: routed.length, updatedAt: Math.max(0, ...routed.map((record) => Number(routeCommandForRecord(record, commands)?.updated_at_ms || 0))) },
  ];
}

function routeSnapshot(record) {
  if (record.__kind === 'thread') {
    return {
      kind: 'inbound_thread',
      id: record.thread_key,
      thread_key: record.thread_key,
      account_key: record.account_key || '',
      subject: record.subject || '',
      unread_count: Number(record.unread_count || 0),
      last_message_at: record.last_message_at || '',
    };
  }
  return {
    kind: 'outbound_message',
    id: record.id,
    campaign_id: record.campaign_id || '',
    recipient_email: record.recipient_email || record.recipient_address_text || '',
    subject: record.subject || '',
    approval_status: record.approval_status || '',
    send_status: record.send_status || '',
  };
}

function safeCommandPart(value) {
  return String(value || '').replace(/[^a-zA-Z0-9_-]+/g, '_').slice(0, 72) || 'record';
}

function buildMailRouteCommands({ batchId, destinationModule, mode = 'handoff', note = '', records = [], actor = {} } = {}) {
  const actorContext = {
    id: String(actor.id || actor.user_id || ''),
    display_name: String(actor.display_name || actor.name || actor.email || ''),
    role: String(actor.role || ''),
  };
  const baseContext = {
    actor: actorContext,
    source_module: 'mail',
    target_module: destinationModule,
    surface: 'mail.bulk.route',
    route_batch_id: batchId,
  };
  const commands = [];
  const genericRecords = [];
  for (const record of records) {
    if (destinationModule === 'support' && record.__kind === 'thread') {
      const sourceId = mailRecordId(record);
      const commandId = `cmd_mail_route_${crypto.randomUUID()}`;
      commands.push({
        id: commandId,
        command_id: commandId,
        module: 'support',
        command_type: 'support.conversation.open_from_thread',
        record_id: sourceId,
        inbound_channel: 'business_os.mail',
        status: 'pending_sync',
        payload: {
          conversation_id: `support_mail_${safeCommandPart(sourceId)}`,
          thread_key: sourceId,
          primary_thread_key: sourceId,
          account_key: record.account_key || '',
          channel: 'email',
          subject: record.subject || '',
          unread_count: Number(record.unread_count || 0),
          route_kind: 'mail.bulk.route',
          route_batch_id: batchId,
          route_mode: mode,
          route_note: note,
          source_module: 'mail',
          target_module: 'support',
          source_record_ids: [sourceId],
          record_snapshot: routeSnapshot(record),
        },
        client_context: baseContext,
      });
    } else {
      genericRecords.push(record);
    }
  }
  for (let index = 0; index < genericRecords.length; index += 100) {
    const chunk = genericRecords.slice(index, index + 100);
    const chunkNumber = Math.floor(index / 100) + 1;
    const commandId = `cmd_mail_route_${crypto.randomUUID()}`;
    const sourceRecordIds = chunk.map(mailRecordId);
    const instruction = note || `Übernimm ${chunk.length} E-Mail-Datensätze aus Mail als ${mode === 'reference' ? 'Referenz' : 'Arbeitsauftrag'}.`;
    commands.push({
      id: commandId,
      command_id: commandId,
      module: destinationModule,
      command_type: 'business_os.chat.task',
      record_id: `${batchId}_${chunkNumber}`,
      inbound_channel: 'business_os.mail',
      status: 'pending_sync',
      payload: {
        title: `${chunk.length} E-Mails aus Mail`,
        prompt: instruction,
        instruction,
        user_message: instruction,
        source_module: 'mail',
        target_module: destinationModule,
        route_kind: 'mail.bulk.route',
        route_batch_id: batchId,
        route_mode: mode,
        source_record_ids: sourceRecordIds,
        record_snapshot: { schema: 'ctox.mail.route-batch.v1', records: chunk.map(routeSnapshot) },
        thread_key: `business-os/mail/routes/${batchId}/${chunkNumber}`,
        response_channel: 'business_os_chat',
        outbound_channel: 'business_os_chat',
        risk_class: 'internal',
      },
      client_context: baseContext,
    });
  }
  return commands;
}

function listBandCounts(records, commands = []) {
  return {
    all: records.length,
    inbound: records.filter((record) => record.__kind === 'thread').length,
    outbound: records.filter((record) => record.__kind === 'outbound').length,
    attention: records.filter((record) => recordNeedsAttention(record) || routeCommandForRecord(record, commands)?.status === 'failed').length,
  };
}

function recordMatchesBand(record, band, messages, commands = []) {
  void messages;
  if (band === 'inbound') return record.__kind === 'thread';
  if (band === 'outbound') return record.__kind === 'outbound';
  if (band === 'attention') return recordNeedsAttention(record) || routeCommandForRecord(record, commands)?.status === 'failed';
  return true;
}

function recordMatchesStatus(record, status, commands = []) {
  if (status === 'unread') return record.__kind === 'thread' && Number(record.unread_count || 0) > 0;
  if (status === 'awaiting') return String(record.approval_status || '').toLowerCase() === 'awaiting_approval';
  if (status === 'routed') return Boolean(routeCommandForRecord(record, commands));
  if (status === 'failed') return String(record.send_status || '').toLowerCase().includes('fail')
    || routeCommandForRecord(record, commands)?.status === 'failed';
  return true;
}

function recordNeedsAttention(record) {
  if (record.__kind === 'thread') return Number(record.unread_count || 0) > 0;
  const approval = String(record.approval_status || '').toLowerCase();
  const send = String(record.send_status || '').toLowerCase();
  return approval === 'awaiting_approval' || send.includes('fail');
}

function recordSubject(record) {
  return String(record.subject || record.recipient_email || record.thread_key || '').toLowerCase();
}

function recordTime(record) {
  return timeOf(record.last_message_at || record.updated_at_ms || record.updated_at || record.created_at_ms || record.external_created_at);
}

function parseRecipientAddresses(value) {
  const matches = String(value || '').match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi) || [];
  return [...new Set(matches.map((address) => address.toLowerCase()).filter(isEmailAddress))];
}

function extractImportRecipients(payload = {}) {
  const chunks = [payload.source?.text || ''];
  for (const file of payload.source?.files || []) {
    if (file.text) chunks.push(file.text);
    else if (file.base64) {
      try {
        const binary = atob(file.base64);
        chunks.push(new TextDecoder().decode(Uint8Array.from(binary, (character) => character.charCodeAt(0))));
      } catch {}
    }
  }
  return parseRecipientAddresses(chunks.join('\n'));
}

// `host` ist der App-Host des Moduls: mail hat keinen Modul-State, deshalb
// wird der Host vom Aufrufer durchgereicht statt global gesucht.
function downloadJson(payload, filename, host) {
  let url = '';
  try {
    url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.rel = 'noopener';
    link.style.display = 'none';
    (host || document.documentElement).append(link);
    link.click();
    link.remove();
  } finally {
    if (url) window.setTimeout(() => URL.revokeObjectURL(url), 4000);
  }
}

function applyStaticLabels(root, t) {
  const labels = {
    '[data-mail-kicker]': t('workspace', 'Arbeitsbereich'),
    '[data-mail-title]': t('mail', 'Mail'),
    '[data-mail-empty-title]': t('emptyTitle', 'Keine E-Mails'),
    '[data-mail-empty-body]': t('emptyBody', 'Nachrichten erscheinen nach der ersten Synchronisierung.'),
    '[data-mail-detail-empty-title]': t('noSelectionTitle', 'Keine Mail ausgewählt'),
    '[data-mail-detail-empty-body]': t('noSelectionBody', 'Wähle eine Nachricht oder Kampagne aus.'),
    '[data-mail-composer-title]': t('compose', 'Neue Mail'),
    '[data-mail-from-label]': t('from', 'Von'),
    '[data-mail-to-label]': t('to', 'An'),
    '[data-mail-group-label]': t('group', 'Gruppe / Kampagne'),
    '[data-mail-subject-label]': t('subject', 'Betreff'),
    '[data-mail-message-label]': t('message', 'Nachricht'),
    '[data-mail-save-draft]': t('saveDraft', 'Als Entwurf speichern'),
    '[data-mail-request-approval]': t('requestApproval', 'Zur Freigabe'),
    '[data-mail-create-group]': t('createGroup', 'Gruppe erstellen'),
    '[data-mail-mailbox-admin-kicker]': t('administration', 'Administration'),
    '[data-mail-mailbox-admin-title]': t('mailSettings', 'Mail-Einstellungen'),
    '[data-mail-domains-label]': t('mailserverDomains', 'Mailserver-Domains'),
    '[data-mail-new-mailbox-label]': t('createMailbox', 'Postfach anlegen'),
    '[data-mail-address-label]': t('emailAddress', 'E-Mail-Adresse'),
    '[data-mail-owner-label]': t('ownerUser', 'Nutzer'),
    '[data-mail-password-label]': t('initialPassword', 'Initialpasswort'),
    '[data-mail-create-mailbox]': t('createMailbox', 'Postfach anlegen'),
    '[data-mail-existing-mailboxes-label]': t('existingMailboxes', 'Vorhandene Postfächer'),
    '[data-mail-mailbox-security]': t('mailboxSecurity', 'Das Passwort wird nach der Einrichtung aus dem Command entfernt.'),
    '[data-mail-reload-mailboxes]': t('reload', 'Neu laden'),
  };
  for (const [selector, text] of Object.entries(labels)) {
    const element = root.querySelector(selector);
    if (element) element.textContent = text;
  }
  const search = root.querySelector('[data-mail-search]');
  if (search) search.placeholder = t('search', 'Mails durchsuchen');
}

function visibleEmailAccounts(accounts, user) {
  const emailAccounts = (accounts || []).filter((account) => account.channel === 'email' && !isDeleted(account));
  if (isGlobalMailAdmin(user)) return emailAccounts.sort(sortAccount);
  return emailAccounts.filter((account) => {
    const profile = accountProfile(account);
    const owner = String(profile.owner_user_id || profile.ownerUserId || '').trim();
    const shared = arrayStrings(profile.shared_user_ids || profile.sharedUserIds || profile.member_user_ids);
    const userIds = new Set([user?.id, user?.user_id, user?.email, user?.login].filter(Boolean).map(String));
    if (!owner) return true;
    if (userIds.has(owner)) return true;
    return shared.some((id) => userIds.has(id));
  }).sort(sortAccount);
}

function visibleMailCampaigns(campaigns, user, accounts, messages) {
  const active = (campaigns || []).filter((campaign) => !isDeleted(campaign));
  if (isGlobalMailAdmin(user)) return active.sort(sortUpdatedDesc);
  const userIds = new Set([user?.id, user?.user_id, user?.email, user?.login].filter(Boolean).map(String));
  const accountKeys = new Set((accounts || []).map((account) => account.account_key));
  const campaignIdsForAccounts = new Set((messages || [])
    .filter((message) => accountKeys.has(message.communication_account_key || message.sender_account_id))
    .map((message) => message.campaign_id));
  return active.filter((campaign) => {
    const owner = String(campaign.owner_id || '').trim();
    const shared = arrayStrings(campaign.payload?.shared_user_ids || campaign.payload?.member_user_ids);
    if (owner && userIds.has(owner)) return true;
    if (shared.some((id) => userIds.has(id))) return true;
    return campaignIdsForAccounts.has(campaign.id);
  }).sort(sortUpdatedDesc);
}

function accountOwnedByCurrentUser(account, user) {
  const owner = String(accountProfile(account).owner_user_id || accountProfile(account).ownerUserId || '').trim();
  if (!owner) return false;
  return [user?.id, user?.user_id, user?.email, user?.login].filter(Boolean).map(String).includes(owner);
}

function isGlobalMailAdmin(user) {
  return ['admin', 'chef', 'owner', 'founder'].includes(String(user?.role || '').toLowerCase());
}

function commandOutcome(command) {
  const result = command?.result ?? command?.payload?.outcome ?? command ?? {};
  return result?.outcome && typeof result.outcome === 'object' ? result.outcome : result;
}

function mailserverDomain(domain) {
  if (typeof domain === 'string') return domain.trim();
  return String(domain?.domain_name || domain?.domain || domain?.name || '').trim();
}

function mailserverUsername(user) {
  if (typeof user === 'string') return user.trim();
  return String(user?.username || user?.email || user?.address || '').trim();
}

function isEmailAddress(value) {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(String(value || '').trim());
}

function accountProfile(account) {
  const profile = account?.profile_json;
  if (!profile) return {};
  if (typeof profile === 'object') return profile;
  try { return JSON.parse(profile); } catch { return {}; }
}

function filterThreadsForFolder(threads, folder, messages) {
  return (threads || []).filter((thread) => {
    const threadMessages = (messages || []).filter((message) => message.thread_key === thread.thread_key);
    if (folder === 'unread') return Number(thread.unread_count || 0) > 0;
    if (folder === 'sent') return threadMessages.some((message) => message.direction === 'outbound');
    return threadMessages.length === 0
      || threadMessages.some((message) => message.direction === 'inbound' || String(message.folder_hint || '').toLowerCase() === 'inbox');
  }).sort((a, b) => timeOf(b.last_message_at) - timeOf(a.last_message_at));
}

function folderCounts(threads, outboundMessages, communicationMessages) {
  return {
    inbox: filterThreadsForFolder(threads, 'inbox', communicationMessages).length,
    unread: filterThreadsForFolder(threads, 'unread', communicationMessages).length,
    sent: filterThreadsForFolder(threads, 'sent', communicationMessages).length,
    drafts: (outboundMessages || []).filter(isDraftMessage).length,
  };
}

function campaignStats(campaignId, messages) {
  const rows = (messages || []).filter((message) => message.campaign_id === campaignId);
  const progress = deriveMailGroupProgress(campaignId, messages);
  return {
    ...progress,
    total: rows.length,
    drafts: rows.filter(isDraftMessage).length,
    awaiting: rows.filter((message) => message.approval_status === 'awaiting_approval').length,
    sent: rows.filter((message) => ['sent', 'accepted'].includes(message.send_status)).length,
    queued: rows.filter((message) => ['queued_for_provider', 'queued'].includes(String(message.send_status || ''))).length,
    delivered: rows.filter((message) => messageDeliveryStatus(message) === 'delivered').length,
    opened: rows.filter((message) => Number(message.payload?.open_count || 0) > 0).length,
    clicked: rows.filter((message) => Number(message.payload?.click_count || 0) > 0).length,
    failed: rows.filter((message) => String(message.send_status || '').includes('fail')).length,
  };
}

function groupKindLabel(kind) {
  return ({
    'outbound-sales': 'Outbound Sales',
    newsletter: 'Newsletter',
    support: 'Support-Topf',
    routing: 'Routing-Topf',
    single: 'Einzel-E-Mail',
    free: 'Freie E-Mail-Gruppe',
  })[kind] || 'E-Mail-Gruppe';
}

function groupContentLabel(mode) {
  return ({
    word: 'Rich Text · Word',
    email_visual: 'HTML-E-Mail bearbeiten',
    email_html: 'HTML bearbeiten',
  })[mode] || 'Inhalt bearbeiten';
}

function buildComposeCommandBundle(input, ids, now) {
  const senderAddress = String(input.accountKey || '').replace(/^email:/, '');
  const engagementPayload = {
    engagement_id: ids.engagementId,
    campaign_id: input.campaignId,
    sender_account_id: input.accountKey,
    contact_email: input.recipient,
    status: 'assigned',
    payload: {
      source: 'mail_app',
      recipient_email: input.recipient,
    },
    created_at_ms: now,
  };
  const messagePayload = {
    message_id: ids.messageId,
    engagement_id: ids.engagementId,
    campaign_id: input.campaignId,
    channel: 'email',
    message_type: 'initial',
    direction: 'outbound',
    sender_account_id: input.accountKey,
    communication_account_key: input.accountKey,
    recipient_email: input.recipient,
    subject: input.subject,
    body_text: input.body,
    body_html: plainTextToEmailHtml(input.body),
    payload: {
      source: 'mail_app',
      sender_address: senderAddress,
      mail_content_revision_id: String(input.contentRevisionId || ''),
      tracking_enabled: true,
    },
    created_at_ms: now,
  };
  return {
    engagement: { commandType: 'outbound.engagement.create', payload: engagementPayload },
    message: { commandType: 'outbound.message.prepare', payload: messagePayload },
    approval: { commandType: 'outbound.message.request_approval', payload: { message_id: ids.messageId } },
  };
}

function plainTextToEmailHtml(text) {
  return `<div style="font-family:Arial,sans-serif;font-size:16px;line-height:1.55;color:#17202a">${escapeHtml(String(text || '')).replace(/\n/g, '<br>')}</div>`;
}

function validateComposeInput(input) {
  if (!input.accountKey) return 'missingSender';
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(input.recipient || '')) return 'missingRecipient';
  if (!String(input.subject || '').trim() && !String(input.body || '').trim()) return 'missingContent';
  return '';
}

function buildSeriesEmailTransferHash({ campaignId = '', recipients = [], subject = '', body = '', sourceModule = 'sellify' } = {}) {
  const params = new URLSearchParams({ action: 'series-email', source_module: sourceModule });
  if (campaignId) params.set('campaign_id', campaignId);
  if (recipients.length) params.set('recipients', recipients.join(','));
  if (subject) params.set('subject', subject);
  if (body) params.set('body', body);
  return `mail?${params.toString()}`;
}

function messageActions(message, t) {
  const approval = String(message.approval_status || 'draft');
  const send = String(message.send_status || 'not_scheduled');
  if (approval === 'awaiting_approval') return [{ id: 'approve', label: t('approve', 'Freigeben'), primary: true }];
  if (approval === 'approved' && ['approved_not_sent', 'not_scheduled'].includes(send)) return [{ id: 'send', label: t('send', 'Senden'), primary: true }];
  if (isDraftMessage(message)) return [{ id: 'request-approval', label: t('requestApproval', 'Zur Freigabe'), primary: true }];
  return [];
}

function isDraftMessage(message) {
  return DRAFT_SEND_STATUSES.has(String(message?.send_status || '').toLowerCase())
    && !['sent', 'accepted', 'queued_for_provider'].includes(String(message?.send_status || '').toLowerCase());
}

function canRequestApproval(message) {
  const approval = String(message?.approval_status || 'draft').toLowerCase();
  return !['approved', 'awaiting_approval', 'rejected'].includes(approval) && isDraftMessage(message);
}

function messageStatusLabel(message, t) {
  const send = String(message.send_status || '').toLowerCase();
  const approval = String(message.approval_status || '').toLowerCase();
  if (Number(message.payload?.click_count || 0) > 0) return 'Link geklickt';
  if (Number(message.payload?.open_count || 0) > 0) return 'Öffnung erfasst';
  if (messageDeliveryStatus(message) === 'delivered') return 'Zugestellt';
  if (send === 'queued_for_provider') return t('queued', 'In Mailserver-Queue');
  if (approval === 'awaiting_approval') return t('awaiting', 'Freigabe');
  if (approval === 'approved') return t('approved', 'Freigegeben');
  if (send.includes('fail')) return t('failed', 'Fehler');
  return t('drafts', 'Entwurf');
}

function messageDeliveryStatus(message) {
  const provider = String(message?.payload?.provider_dispatch_status || '').toLowerCase();
  const send = String(message?.send_status || '').toLowerCase();
  if (provider === 'delivered' || ['sent', 'accepted', 'delivered'].includes(send)) return 'delivered';
  if (provider.includes('fail') || send.includes('fail') || send === 'bounced') return 'failed';
  if (['queued_for_provider', 'queued'].includes(send)) return 'queued';
  return 'pending';
}

function messageProgressModel(message) {
  const approval = String(message?.approval_status || 'draft').toLowerCase();
  const send = String(message?.send_status || '').toLowerCase();
  const delivery = messageDeliveryStatus(message);
  const opened = Number(message?.payload?.open_count || 0) > 0;
  const clicked = Number(message?.payload?.click_count || 0) > 0;
  const trackingEnabled = message?.payload?.tracking_enabled === true;
  const queued = ['queued_for_provider', 'queued'].includes(send) || ['delivered', 'failed'].includes(delivery);
  const approved = approval === 'approved' || queued;

  let currentIndex = 0;
  let currentState = 'active';
  if (clicked) {
    currentIndex = 5;
    currentState = 'done';
  } else if (opened) {
    currentIndex = 4;
    currentState = 'done';
  } else if (delivery === 'delivered') {
    currentIndex = 3;
    currentState = 'done';
  } else if (delivery === 'failed') {
    currentIndex = 3;
    currentState = 'failed';
  } else if (queued) {
    currentIndex = 2;
  } else if (approved) {
    currentIndex = 2;
  } else if (approval === 'awaiting_approval') {
    currentIndex = 1;
  }

  const labels = [
    ['Entwurf erstellt', 'edit'],
    [approved ? 'Freigegeben' : 'Freigabe ausstehend', approved ? 'check' : 'clock'],
    [queued ? 'An Mailserver übergeben' : 'Versand ausstehend', 'send'],
    [delivery === 'failed' ? 'Zustellung fehlgeschlagen' : delivery === 'delivered' ? 'Zugestellt' : 'Zustellung ausstehend', delivery === 'failed' ? 'warning' : 'check'],
    [opened ? 'Öffnung erfasst' : trackingEnabled ? 'Keine Öffnung erfasst' : 'Öffnungstracking nicht aktiv', 'eye'],
    [clicked ? 'Link geklickt' : trackingEnabled ? 'Kein Linkklick erfasst' : 'Klicktracking nicht aktiv', 'link'],
  ];
  const steps = labels.map(([label, icon], index) => ({
    label,
    icon,
    state: index < currentIndex ? 'done' : index === currentIndex ? currentState : 'pending',
  }));
  const percent = Math.round((currentIndex / (steps.length - 1)) * 100);
  const currentLabel = labels[currentIndex]?.[0] || labels[0][0];
  return {
    steps,
    percent,
    currentLabel,
    ariaLabel: `Versandfortschritt: ${currentLabel}. ${percent} Prozent.`,
  };
}

function messageEventTimeline(message) {
  const payload = message?.payload || {};
  const events = [{ label: 'Vorbereitet', detail: formatRecordTime(message.created_at_ms), state: 'done' }];
  const approval = String(message.approval_status || 'draft');
  events.push({
    label: approval === 'approved' ? 'Freigegeben' : approval === 'awaiting_approval' ? 'Freigabe ausstehend' : 'Noch nicht freigegeben',
    detail: formatRecordTime(message.updated_at_ms || message.created_at_ms),
    state: approval === 'approved' ? 'done' : 'pending',
  });
  if (payload.provider_queued_at_ms) {
    events.push({ label: 'An Mailserver übergeben', detail: formatRecordTime(payload.provider_queued_at_ms), state: 'done' });
  }
  const delivery = messageDeliveryStatus(message);
  if (delivery === 'delivered') {
    events.push({ label: 'Zugestellt', detail: formatRecordTime(payload.delivered_at_ms || payload.provider_completed_at_ms || message.sent_at_ms), state: 'done' });
  } else if (delivery === 'failed') {
    events.push({ label: 'Zustellung fehlgeschlagen', detail: payload.provider_error_text || payload.last_send_error || 'Der Mailserver hat einen permanenten Fehler gemeldet.', state: 'failed' });
  } else if (delivery === 'queued') {
    events.push({ label: 'Zustellung läuft', detail: 'Wartet auf das terminale SMTP-Ergebnis', state: 'active' });
  }
  if (Number(payload.open_count || 0) > 0) {
    events.push({ label: `Öffnung erfasst · ${payload.open_count}×`, detail: formatRecordTime(payload.last_opened_at_ms), state: 'done' });
  } else if (payload.tracking_enabled) {
    events.push({ label: 'Noch keine Öffnung erfasst', detail: 'Tracking ist aktiv; Datenschutz-Proxys können das Ergebnis beeinflussen.', state: 'pending' });
  }
  if (Number(payload.click_count || 0) > 0) {
    events.push({ label: `Link geklickt · ${payload.click_count}×`, detail: formatRecordTime(payload.last_clicked_at_ms), state: 'done' });
  }
  return events;
}

function latestMessageForThread(threadKey, messages) {
  return (messages || [])
    .filter((message) => message.thread_key === threadKey)
    .sort((a, b) => timeOf(b.external_created_at) - timeOf(a.external_created_at))[0] || null;
}

function renderMetric(value, label) {
  return `<div class="mail-metric"><strong>${Number(value || 0)}</strong><span>${escapeHtml(label)}</span></div>`;
}

async function readAll(collection) {
  if (!collection) return [];
  try {
    const docs = await collection.find().exec();
    return docs.map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
  } catch (error) {
    if (isTransientCollectionReadError(error)) return [];
    console.warn('[mail] collection read failed', error);
    return [];
  }
}

function isTransientCollectionReadError(error) {
  const message = String(error?.message || error || '');
  return /QUERY_CANCELLED|replication-cancel|WebRTC replication cancelled|IDBDatabase.*closing|database connection is closing|collection is closed|closed collection|RxDB Error-Code: COL21/i.test(message);
}

async function insertCollectionRecord(collection, record) {
  if (!collection) throw new Error('Required mail collection is unavailable');
  if (collection.insert) return collection.insert(record);
  throw new Error('Required mail collection is read-only');
}

async function upsertCollectionRecord(collection, record) {
  if (collection.incrementalUpsert) return collection.incrementalUpsert(record);
  if (collection.upsert) return collection.upsert(record);
  const existing = await collection.findOne(record.id).exec();
  if (existing?.incrementalPatch) return existing.incrementalPatch(record);
  if (existing?.patch) return existing.patch(record);
  return collection.insert(record);
}

async function ensureStyles() {
  const version = String(import.meta.url).split('?v=')[1] || STYLE_BUILD;
  const href = new URL('./index.css', import.meta.url);
  href.searchParams.set('v', version);
  let link = document.querySelector('link[data-mail-style]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.mailStyle = 'true';
    document.head.append(link);
  }
  if (link.href !== href.href) link.href = href.href;
}

function option(value, label) {
  const element = document.createElement('option');
  element.value = value;
  element.textContent = label;
  return element;
}

function splitAddresses(value) {
  return String(value || '').split(/[;,]/).map((item) => item.trim()).filter(Boolean);
}

function arrayStrings(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (typeof value === 'string') return splitAddresses(value);
  return [];
}

function isDeleted(value) {
  return value?._deleted === true || value?.is_deleted === true;
}

function sortUpdatedDesc(a, b) {
  return Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0);
}

function sortAccount(a, b) {
  return String(a.address || a.account_key || '').localeCompare(String(b.address || b.account_key || ''));
}

function timeOf(value) {
  if (typeof value === 'number') return value;
  const parsed = Date.parse(String(value || ''));
  return Number.isFinite(parsed) ? parsed : 0;
}

function formatRecordTime(value) {
  const ms = timeOf(value);
  if (!ms) return '';
  const date = new Date(ms);
  const now = new Date();
  if (date.toDateString() === now.toDateString()) {
    return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(date);
  }
  return new Intl.DateTimeFormat(undefined, { day: '2-digit', month: '2-digit', year: date.getFullYear() === now.getFullYear() ? undefined : '2-digit' }).format(date);
}

function replySubject(subject) {
  const clean = String(subject || '').trim();
  if (!clean) return 'Re:';
  return /^re:/i.test(clean) ? clean : `Re: ${clean}`;
}

function compact(value) {
  return String(value || '').replace(/\s+/g, ' ').trim();
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, Number(value) || min));
}

function sleep(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function stableJson(value) {
  const normalize = (item) => {
    if (Array.isArray(item)) return item.map(normalize);
    if (!item || typeof item !== 'object') return item;
    return Object.fromEntries(Object.keys(item).sort().map((key) => [key, normalize(item[key])]));
  };
  return JSON.stringify(normalize(value));
}

async function sha256Text(value) {
  const bytes = new TextEncoder().encode(String(value ?? ''));
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[char]);
}

function escapeAttribute(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

export const __mailTestHooks = {
  visibleEmailAccounts,
  visibleMailCampaigns,
  filterThreadsForFolder,
  folderCounts,
  campaignStats,
  messageDeliveryStatus,
  messageProgressModel,
  messageEventTimeline,
  plainTextToEmailHtml,
  buildComposeCommandBundle,
  mailActionIcon,
  validateComposeInput,
  messageActions,
  isDraftMessage,
  replySubject,
  isGlobalMailAdmin,
  commandOutcome,
  mailserverDomain,
  mailserverUsername,
  isEmailAddress,
  parseRecipientAddresses,
  buildSeriesEmailTransferHash,
  listBandCounts,
  mailRecordKey,
  mailQueueDefinitions,
  routeCommandForRecord,
  routeDestinationsFromCatalog,
  buildMailRouteCommands,
  stableJson,
  sha256Text,
  isTransientCollectionReadError,
};
