// Active outreach workflow for the outbound module.
// Loaded by outbound/index.js. Operates on the shared `state` object.
// Implements lead queue, engagement cockpit, approval inbox, ready-to-send queue,
// reply handling and the surrounding command dispatch wiring.

let stateRef = null;
let translate = (key, fallback) => fallback ?? key;
let escapeFn = (value) => String(value ?? '');
let triggerRender = () => {};

const REPLY_CLASSES = [
  'unclear',
  'positive',
  'negative',
  'objection',
  'out_of_office',
  'referral',
  'unsubscribe',
];

const DRAFT_KINDS = ['initial', 'followup', 'reply', 'scheduling'];
// Limit how many engagements / messages we render per view at once to keep large
// datasets responsive. Users can switch through pages from the footer.
const ACTIVE_OUTREACH_PAGE_SIZE = 100;

export function configureActiveOutreach({ state, t, escapeHtml, rerender }) {
  stateRef = state;
  translate = typeof t === 'function' ? t : translate;
  escapeFn = typeof escapeHtml === 'function' ? escapeHtml : escapeFn;
  triggerRender = typeof rerender === 'function' ? rerender : triggerRender;
  if (!state.activeOutreach) {
    state.activeOutreach = {
      view: 'lead_queue',
      selectedEngagementId: '',
      selectedMessageId: '',
      sender: { engagementId: '', accountKey: '', open: false },
      messageEdits: new Map(),
      busyMessageIds: new Set(),
      busyEngagementIds: new Set(),
      error: null,
      schedulerInterval: null,
    };
  }
  // Periodically poke the backend scheduler so auto_draft_pending/ooo_retry_at_ms
  // flags translate into approval-gated drafts without manual UI intervention.
  if (!state.activeOutreach.schedulerInterval && typeof window !== 'undefined') {
    state.activeOutreach.schedulerInterval = window.setInterval(() => {
      runSchedulerTickQuiet();
    }, 5 * 60 * 1000);
  }
  state.engagements = state.engagements || [];
  state.engagementMessages = state.engagementMessages || [];
  state.engagementApprovals = state.engagementApprovals || [];
  state.engagementSequences = state.engagementSequences || [];
  state.senderAssignments = state.senderAssignments || [];
  state.meetingRequests = state.meetingRequests || [];
  state.suppressionEntries = state.suppressionEntries || [];
  state.accountLimits = state.accountLimits || [];
  state.outboundSkillbooks = state.outboundSkillbooks || [];
  state.outboundLetterTemplates = state.outboundLetterTemplates || [];
}

async function findAll(collection) {
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs
    .map((doc) => (doc.toJSON ? doc.toJSON() : doc))
    .sort((a, b) => (b.updated_at_ms || 0) - (a.updated_at_ms || 0));
}

export async function loadActiveOutreachData() {
  if (!stateRef?.ctx?.db?.raw) return;
  const raw = stateRef.ctx.db.raw;
  const [
    engagements,
    messages,
    approvals,
    sequences,
    senderAssignments,
    meetingRequests,
    suppressionEntries,
    accountLimits,
    skillbooks,
    letterTemplates,
  ] = await Promise.all([
    findAll(raw.outbound_engagements),
    findAll(raw.outbound_messages),
    findAll(raw.outbound_approvals),
    findAll(raw.outbound_sequences),
    findAll(raw.outbound_sender_assignments),
    findAll(raw.outbound_meeting_requests),
    findAll(raw.outbound_suppression_entries),
    findAll(raw.outbound_account_limits),
    findAll(raw.outbound_skillbooks),
    findAll(raw.outbound_letter_templates),
  ]);
  stateRef.engagements = engagements;
  stateRef.engagementMessages = messages;
  stateRef.engagementApprovals = approvals;
  stateRef.engagementSequences = sequences;
  stateRef.senderAssignments = senderAssignments;
  stateRef.meetingRequests = meetingRequests;
  stateRef.suppressionEntries = suppressionEntries;
  stateRef.accountLimits = accountLimits;
  stateRef.outboundSkillbooks = skillbooks;
  stateRef.outboundLetterTemplates = letterTemplates;
}

export function activeOutreachCounts(campaignId) {
  const engagements = stateRef.engagements.filter((e) => e.campaign_id === campaignId);
  const messages = stateRef.engagementMessages.filter((m) => m.campaign_id === campaignId);
  return {
    leadQueue: countLeadQueue(campaignId),
    engagements: engagements.filter((e) =>
      ['ready_for_assignment', 'assigned', 'draft_prepared', 'awaiting_approval', 'approved_for_send', 'scheduled_to_send'].includes(e.status || '')
    ).length,
    approvalInbox: messages.filter((m) => (m.approval_status || '') === 'awaiting_approval').length,
    readyToSend: messages.filter((m) => (m.approval_status || '') === 'approved' && !['sent', 'queued_for_provider'].includes(m.send_status || '')).length,
    replies: engagements.filter((e) => (e.status || '') === 'reply_received').length,
    done: engagements.filter((e) => ['closed', 'meeting_booked'].includes(e.status || '')).length,
  };
}

function countLeadQueue(campaignId) {
  const engagedIds = new Set(
    stateRef.engagements
      .filter((e) => e.campaign_id === campaignId)
      .map((e) => e.payload?.pipeline_id || e.pipeline_id || '')
      .filter(Boolean),
  );
  const qualifiedLeads = (stateRef.pipeline || []).filter((item) => {
    if (item.campaign_id !== campaignId) return false;
    if (engagedIds.has(item.id)) return false;
    const status = (item.lead_status || item.qualification_status || '').toLowerCase();
    return status === 'qualified' || status === 'lead_qualified';
  });
  return qualifiedLeads.length;
}

export function renderActiveOutreachShell(campaign) {
  if (!campaign) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('selectCampaignFirst', 'Bitte zuerst eine Campaign wählen.'))}</div>`;
  }
  const counts = activeOutreachCounts(campaign.id);
  const view = stateRef.activeOutreach.view || 'lead_queue';
  const tabs = [
    { key: 'lead_queue', label: translate('leadQueue', 'Lead Queue'), count: counts.leadQueue },
    { key: 'engagements', label: translate('activeEngagements', 'Aktive Engagements'), count: counts.engagements },
    { key: 'approval_inbox', label: translate('approvalInbox', 'Freigabe-Inbox'), count: counts.approvalInbox },
    { key: 'ready_to_send', label: translate('readyToSend', 'Versand-Queue'), count: counts.readyToSend },
    { key: 'replies', label: translate('replies', 'Antworten'), count: counts.replies },
    { key: 'done', label: translate('done', 'Abgeschlossen'), count: counts.done },
    { key: 'settings', label: translate('skillSettings', 'Skills & Vorlagen'), count: (stateRef.outboundSkillbooks?.length || 0) + (stateRef.outboundLetterTemplates?.length || 0) },
  ];
  const errorBanner = stateRef.activeOutreach.error
    ? `<div class="outbound-outreach-error" role="alert" aria-live="assertive">${escapeFn(stateRef.activeOutreach.error)} <button type="button" data-action="ao-dismiss-error" aria-label="${escapeFn(translate('cancel', 'Schließen'))}">×</button></div>`
    : '';
  return `
    <section class="outbound-outreach" data-outreach-campaign="${escapeFn(campaign.id)}">
      <header class="outbound-outreach-head">
        <div>
          <h3>${escapeFn(translate('activeOutreach', 'Aktive Kommunikation'))}</h3>
          <p>${escapeFn(translate('activeOutreachIntro', 'CTOX bereitet Nachrichten, Antworten und Termine vor. Versand erst nach expliziter Freigabe.'))}</p>
        </div>
        <div class="outbound-outreach-head-actions">
          <button type="button" class="outbound-button" data-action="ao-reconcile-provider" title="${escapeFn(translate('reconcileProviderHint', 'Status der Mailserver-Queue mit Outbound-Messages abgleichen'))}">${escapeFn(translate('reconcileProvider', 'Provider-Status abgleichen'))}</button>
          <button type="button" class="outbound-button" data-action="ao-scheduler-tick" title="${escapeFn(translate('schedulerTickHint', 'Fällige Engagements verarbeiten: Drafts vorbereiten, Suppressed schließen'))}">${escapeFn(translate('schedulerTick', 'Scheduler-Tick'))}</button>
          <button type="button" class="outbound-button" data-action="ao-audit-export" title="${escapeFn(translate('auditExportHint', 'Vollstaendigen JSON-Export der Outbound-Daten dieser Campaign laden'))}">${escapeFn(translate('auditExport', 'Audit-Export'))}</button>
        </div>
      </header>
      ${errorBanner}
      <nav class="outbound-outreach-tabs" role="tablist">
        ${tabs.map((tab) => renderOutreachTab(tab, view)).join('')}
      </nav>
      <div class="outbound-outreach-body" role="tabpanel">
        ${renderOutreachView(campaign, view)}
      </div>
      ${renderSenderModal(campaign)}
    </section>
  `;
}

function renderOutreachTab(tab, view) {
  const active = view === tab.key;
  return `
    <button
      type="button"
      role="tab"
      aria-selected="${active}"
      class="outbound-outreach-tab${active ? ' is-active' : ''}"
      data-action="ao-view"
      data-view="${escapeFn(tab.key)}"
    >
      <span>${escapeFn(tab.label)}</span>
      <em>${escapeFn(tab.count)}</em>
    </button>
  `;
}

function renderOutreachView(campaign, view) {
  switch (view) {
    case 'engagements':
      return renderEngagementsView(campaign);
    case 'approval_inbox':
      return renderApprovalInbox(campaign);
    case 'ready_to_send':
      return renderReadyToSend(campaign);
    case 'replies':
      return renderRepliesView(campaign);
    case 'done':
      return renderDoneView(campaign);
    case 'settings':
      return renderSkillSettings(campaign);
    case 'lead_queue':
    default:
      return renderLeadQueue(campaign);
  }
}

function renderSkillSettings(campaign) {
  const skillbooks = stateRef.outboundSkillbooks || [];
  const templates = (stateRef.outboundLetterTemplates || []).filter(
    (tpl) => !tpl.campaign_id || tpl.campaign_id === campaign.id,
  );
  return `
    <section class="outbound-outreach-settings">
      <header>
        <h4>${escapeFn(translate('skillbooks', 'Skillbooks'))}</h4>
        <button type="button" class="outbound-button" data-action="ao-seed-skillbooks">${escapeFn(translate('seedDefaults', 'Defaults laden'))}</button>
      </header>
      ${skillbooks.length === 0
        ? `<p class="outbound-outreach-muted">${escapeFn(translate('noSkillbooksYet', 'Noch keine Skillbooks geseedet. Klick "Defaults laden".'))}</p>`
        : skillbooks.map((sb) => renderSkillbookEditor(sb)).join('')}
      <header>
        <h4>${escapeFn(translate('letterTemplates', 'Briefvorlagen'))}</h4>
        <button type="button" class="outbound-button" data-action="ao-new-letter-template" data-campaign-id="${escapeFn(campaign.id)}">${escapeFn(translate('newTemplate', 'Neue Vorlage'))}</button>
      </header>
      ${templates.length === 0
        ? `<p class="outbound-outreach-muted">${escapeFn(translate('noLetterTemplatesYet', 'Noch keine Briefvorlagen für diese Campaign.'))}</p>`
        : templates.map((tpl) => renderLetterTemplateEditor(tpl)).join('')}
    </section>
  `;
}

function renderSkillbookEditor(sb) {
  const rules = Array.isArray(sb.non_negotiable_rules) ? sb.non_negotiable_rules.join('\n') : '';
  const workflow = Array.isArray(sb.workflow_backbone) ? sb.workflow_backbone.join('\n') : '';
  return `
    <article class="outbound-outreach-skillbook-card" data-skillbook-id="${escapeFn(sb.id || sb.skillbook_id)}">
      <header>
        <strong>${escapeFn(sb.title || sb.skillbook_id || sb.id)}</strong>
        <small>${escapeFn(translate('version', 'Version'))} ${escapeFn(sb.version_number || 1)}</small>
      </header>
      <label>
        <span>${escapeFn(translate('mission', 'Mission'))}</span>
        <textarea rows="2" data-sb-field="mission">${escapeFn(sb.mission || '')}</textarea>
      </label>
      <label>
        <span>${escapeFn(translate('nonNegotiableRules', 'Non-negotiable rules'))} (${escapeFn(translate('oneRulePerLine', 'eine pro Zeile'))})</span>
        <textarea rows="4" data-sb-field="rules">${escapeFn(rules)}</textarea>
      </label>
      <label>
        <span>${escapeFn(translate('workflowBackbone', 'Workflow'))} (${escapeFn(translate('oneStepPerLine', 'ein Schritt pro Zeile'))})</span>
        <textarea rows="4" data-sb-field="workflow">${escapeFn(workflow)}</textarea>
      </label>
      <footer>
        <button type="button" class="outbound-button primary" data-action="ao-save-skillbook" data-skillbook-id="${escapeFn(sb.id || sb.skillbook_id)}">${escapeFn(translate('saveDraft', 'Speichern'))}</button>
      </footer>
    </article>
  `;
}

function renderLetterTemplateEditor(tpl) {
  return `
    <article class="outbound-outreach-template-card" data-template-id="${escapeFn(tpl.id)}">
      <header>
        <strong>${escapeFn(tpl.title || tpl.id)}</strong>
        <small>${escapeFn(translate('version', 'Version'))} ${escapeFn(tpl.version_number || 1)}</small>
      </header>
      <label>
        <span>${escapeFn(translate('title', 'Titel'))}</span>
        <input type="text" value="${escapeFn(tpl.title || '')}" data-tpl-field="title" />
      </label>
      <label>
        <span>${escapeFn(translate('salutation', 'Anrede'))}</span>
        <input type="text" value="${escapeFn(tpl.salutation || '')}" data-tpl-field="salutation" />
      </label>
      <label>
        <span>${escapeFn(translate('bodyTemplate', 'Body-Vorlage'))} <small>${escapeFn(translate('bodyTemplateHint', 'wird zwischen Anrede und Schluss gepackt'))}</small></span>
        <textarea rows="6" data-tpl-field="body_template">${escapeFn(tpl.body_template || '')}</textarea>
      </label>
      <label>
        <span>${escapeFn(translate('closing', 'Schluss'))}</span>
        <textarea rows="2" data-tpl-field="closing">${escapeFn(tpl.closing || '')}</textarea>
      </label>
      <footer>
        <button type="button" class="outbound-button primary" data-action="ao-save-template" data-template-id="${escapeFn(tpl.id)}">${escapeFn(translate('saveDraft', 'Speichern'))}</button>
      </footer>
    </article>
  `;
}

function renderLeadQueue(campaign) {
  const engagedPipelineIds = new Set(
    stateRef.engagements
      .filter((e) => e.campaign_id === campaign.id)
      .map((e) => e.payload?.pipeline_id || e.pipeline_id || ''),
  );
  const leads = (stateRef.pipeline || []).filter((item) => {
    if (item.campaign_id !== campaign.id) return false;
    if (engagedPipelineIds.has(item.id)) return false;
    const status = (item.lead_status || item.qualification_status || '').toLowerCase();
    return status === 'qualified' || status === 'lead_qualified';
  });
  if (leads.length === 0) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('noQualifiedLeads', 'Keine qualifizierten Leads warten auf Engagement.'))}</div>`;
  }
  const defaultMailbox = campaign.communication_account_key || campaign.payload?.communication_account_key || '';
  const defaultChannel = campaign.payload?.active_outreach?.default_channel || 'email';
  return `
    <table class="outbound-outreach-table">
      <thead>
        <tr>
          <th>${escapeFn(translate('lead', 'Lead'))}</th>
          <th>${escapeFn(translate('company', 'Unternehmen'))}</th>
          <th>${escapeFn(translate('contact', 'Kontakt'))}</th>
          <th>${escapeFn(translate('channel', 'Kanal'))}</th>
          <th>${escapeFn(translate('actions', 'Aktionen'))}</th>
        </tr>
      </thead>
      <tbody>
        ${leads.map((lead) => renderLeadQueueRow(lead, campaign, defaultMailbox, defaultChannel)).join('')}
      </tbody>
    </table>
  `;
}

function renderLeadQueueRow(lead, campaign, defaultMailbox, defaultChannel) {
  const contactName = lead.contact_name || lead.payload?.contact_name || '—';
  const contactEmail = lead.contact_email || lead.payload?.contact_email || '';
  const channelLabel = defaultChannel === 'physical_letter' ? translate('physicalLetter', 'Brief') : translate('email', 'E-Mail');
  const mailboxBadge = defaultMailbox
    ? `<small class="outbound-outreach-mailbox">${escapeFn(defaultMailbox.replace(/^email:/, ''))}</small>`
    : `<small class="outbound-outreach-mailbox is-missing">${escapeFn(translate('noMailbox', 'kein Postfach'))}</small>`;
  return `
    <tr data-lead-id="${escapeFn(lead.id)}">
      <td>${escapeFn(lead.lead_name || lead.payload?.lead_name || lead.id)}</td>
      <td>${escapeFn(lead.company_name || lead.payload?.company_name || '—')}</td>
      <td>
        <div>${escapeFn(contactName)}</div>
        <small>${escapeFn(contactEmail)}</small>
      </td>
      <td>${escapeFn(channelLabel)} ${mailboxBadge}</td>
      <td class="outbound-outreach-actions-cell">
        <button type="button" class="outbound-button primary" data-action="ao-start-engagement" data-lead-id="${escapeFn(lead.id)}">${escapeFn(translate('startEngagement', 'Engagement starten'))}</button>
        <button type="button" class="outbound-button" data-action="ao-auto-draft" data-lead-id="${escapeFn(lead.id)}">${escapeFn(translate('autoDraft', 'Auto-Draft'))}</button>
      </td>
    </tr>
  `;
}

function renderEngagementsView(campaign) {
  const engagements = stateRef.engagements
    .filter((e) => e.campaign_id === campaign.id)
    .filter((e) => !['closed', 'meeting_booked'].includes(e.status || ''));
  if (engagements.length === 0) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('noActiveEngagements', 'Keine aktiven Engagements.'))}</div>`;
  }
  const slice = engagements.slice(0, ACTIVE_OUTREACH_PAGE_SIZE);
  const truncated = engagements.length > ACTIVE_OUTREACH_PAGE_SIZE;
  const selectedId = stateRef.activeOutreach.selectedEngagementId;
  const selected = slice.find((e) => e.id === selectedId) || slice[0];
  return `
    <div class="outbound-outreach-split">
      <ol class="outbound-outreach-list" role="listbox">
        ${slice.map((e) => renderEngagementListItem(e, selected?.id === e.id)).join('')}
        ${truncated ? `<li class="outbound-outreach-muted">${escapeFn(translate('truncatedList', 'Liste auf {0} eingegrenzt. Insgesamt {1} aktive Engagements.', ACTIVE_OUTREACH_PAGE_SIZE, engagements.length))}</li>` : ''}
      </ol>
      <article class="outbound-outreach-detail">
        ${renderEngagementDetail(campaign, selected)}
      </article>
    </div>
  `;
}

function renderEngagementListItem(engagement, isSelected) {
  const lastMessage = lastMessageForEngagement(engagement.id);
  const lastSubject = lastMessage?.subject || lastMessage?.payload?.subject || '';
  const status = engagement.status || '—';
  return `
    <li>
      <button
        type="button"
        class="outbound-outreach-list-item${isSelected ? ' is-active' : ''}"
        data-action="ao-select-engagement"
        data-id="${escapeFn(engagement.id)}"
        aria-pressed="${isSelected}"
      >
        <strong>${escapeFn(engagement.payload?.contact_name || engagement.payload?.company_name || engagement.id)}</strong>
        <span class="outbound-outreach-status outbound-outreach-status-${escapeFn(status)}">${escapeFn(prettyStatus(status))}</span>
        <em>${escapeFn(lastSubject || translate('noMessageYet', 'Noch keine Nachricht'))}</em>
      </button>
    </li>
  `;
}

function renderEngagementDetail(campaign, engagement) {
  if (!engagement) return `<div class="outbound-outreach-empty">${escapeFn(translate('selectEngagement', 'Engagement wählen'))}</div>`;
  const messages = stateRef.engagementMessages
    .filter((m) => m.engagement_id === engagement.id)
    .sort((a, b) => (a.created_at_ms || 0) - (b.created_at_ms || 0));
  const sender = engagement.sender_account_id || engagement.payload?.sender_account_id || '';
  const senderAddress = sender.replace(/^email:/, '');
  const lastMessage = messages[messages.length - 1];
  const paused = (engagement.status || '') === 'paused';
  const nextStep = computeNextStep(engagement, lastMessage);
  const replyClass = engagement.payload?.reply_classification || '';
  const conversationsLink = buildConversationsDeepLink(campaign, engagement, lastMessage);
  const sequenceMismatch = detectSequenceVersionMismatch(engagement);
  return `
    <header class="outbound-outreach-detail-head">
      <div>
        <h4>${escapeFn(engagement.payload?.contact_name || engagement.payload?.company_name || engagement.id)}</h4>
        <small>${escapeFn(engagement.payload?.contact_email || '')}</small>
      </div>
      <div class="outbound-outreach-detail-meta">
        <span class="outbound-outreach-status outbound-outreach-status-${escapeFn(engagement.status || 'unknown')}">${escapeFn(prettyStatus(engagement.status || 'unknown'))}</span>
        <small>${escapeFn(senderAddress || translate('noSender', 'Kein Sender'))}</small>
        ${conversationsLink ? `<a class="outbound-outreach-conv-link" href="${escapeFn(conversationsLink)}" target="_top">${escapeFn(translate('openInConversations', 'In Conversations öffnen'))}</a>` : ''}
      </div>
    </header>
    <section class="outbound-outreach-detail-section outbound-outreach-detail-next">
      <h5>${escapeFn(translate('nextStep', 'Nächster Schritt'))}</h5>
      <p>${escapeFn(nextStep.label)}</p>
      ${engagement.next_action_at_ms ? `<p class="outbound-outreach-muted">${escapeFn(translate('whyNow', 'Geplant für'))} ${escapeFn(formatTimestamp(engagement.next_action_at_ms))}</p>` : ''}
      <div class="outbound-outreach-detail-actions">
        ${nextStep.actions.map((action) => renderActionButton(action, engagement, lastMessage)).join('')}
        ${paused ? '' : `<button type="button" class="outbound-button" data-action="ao-pause-engagement" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('pause', 'Pausieren'))}</button>`}
        ${paused ? `<button type="button" class="outbound-button" data-action="ao-resume-engagement" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('resume', 'Fortsetzen'))}</button>` : ''}
        <button type="button" class="outbound-button" data-action="ao-close-engagement" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('close', 'Schließen'))}</button>
        <button type="button" class="outbound-button" data-action="ao-open-sender" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('changeSender', 'Sender ändern'))}</button>
      </div>
    </section>
    ${sequenceMismatch ? `<section class="outbound-outreach-detail-section outbound-outreach-sequence-warning"><h5>${escapeFn(translate('sequenceUpdated', 'Sequenz wurde aktualisiert'))}</h5><p>${escapeFn(translate('sequenceMismatchDescription', 'Dieses Engagement läuft mit Sequenz-Version {0}. Aktuelle Version: {1}.', sequenceMismatch.current, sequenceMismatch.latest))}</p><button type="button" class="outbound-button primary" data-action="ao-reapply-sequence" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('reapplySequence', 'Aktuelle Sequenz anwenden'))}</button></section>` : ''}
    ${renderSkillbookSnapshotSection(lastMessage)}
    ${replyClass ? `<section class="outbound-outreach-detail-section"><h5>${escapeFn(translate('replyClass', 'Antwort-Klassifikation'))}</h5><p><strong>${escapeFn(prettyReplyClass(replyClass))}</strong></p></section>` : ''}
    <section class="outbound-outreach-detail-section">
      <h5>${escapeFn(translate('timeline', 'Timeline'))}</h5>
      ${renderEngagementTimeline(engagement, messages)}
    </section>
  `;
}

function renderActionButton(action, engagement, lastMessage) {
  switch (action) {
    case 'auto_draft_initial':
      return `<button type="button" class="outbound-button primary" data-action="ao-draft-initial" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('autoDraftInitial', 'Initial-Mail vorbereiten'))}</button>`;
    case 'auto_draft_followup':
      return `<button type="button" class="outbound-button primary" data-action="ao-draft-followup" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('autoDraftFollowup', 'Follow-up vorbereiten'))}</button>`;
    case 'auto_draft_reply':
      return `<button type="button" class="outbound-button primary" data-action="ao-draft-reply" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('autoDraftReply', 'Antwort vorbereiten'))}</button>`;
    case 'auto_draft_scheduling':
      return `<button type="button" class="outbound-button primary" data-action="ao-draft-scheduling" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('autoDraftScheduling', 'Terminantwort vorbereiten'))}</button>`;
    case 'request_approval':
      return lastMessage
        ? `<button type="button" class="outbound-button" data-action="ao-request-approval" data-message-id="${escapeFn(lastMessage.id)}">${escapeFn(translate('requestApproval', 'Freigabe anfordern'))}</button>`
        : '';
    case 'review_draft':
      return lastMessage
        ? `<button type="button" class="outbound-button" data-action="ao-view-message" data-message-id="${escapeFn(lastMessage.id)}">${escapeFn(translate('reviewDraft', 'Draft prüfen'))}</button>`
        : '';
    default:
      return '';
  }
}

function renderEngagementTimeline(engagement, messages) {
  if (messages.length === 0) {
    return `<p class="outbound-outreach-muted">${escapeFn(translate('timelineEmpty', 'Noch keine Aktivität.'))}</p>`;
  }
  const approvals = stateRef.engagementApprovals.filter((a) => a.engagement_id === engagement.id);
  const entries = [];
  if (engagement.payload?.reply_classification && engagement.payload?.reply_matched_at_ms) {
    entries.push({
      ts: engagement.payload.reply_matched_at_ms,
      kind: 'reply',
      label: translate('replyReceived', 'Antwort erhalten'),
      detail: prettyReplyClass(engagement.payload.reply_classification),
    });
  }
  for (const msg of messages) {
    entries.push({
      ts: msg.created_at_ms || 0,
      kind: 'draft',
      label: translate('draftCreated', 'Draft erstellt'),
      subject: msg.subject || msg.payload?.subject || '',
      msgId: msg.id,
    });
    if (msg.approval_status === 'awaiting_approval') {
      entries.push({ ts: msg.updated_at_ms || 0, kind: 'awaiting', label: translate('awaitingApproval', 'Freigabe ausstehend'), msgId: msg.id });
    }
    if (msg.approval_status === 'approved') {
      entries.push({ ts: msg.updated_at_ms || 0, kind: 'approved', label: translate('approved', 'Freigegeben'), msgId: msg.id });
    }
    if (msg.send_status === 'queued_for_provider') {
      entries.push({ ts: msg.updated_at_ms || 0, kind: 'queued', label: translate('queuedForProvider', 'In Mailserver-Queue'), msgId: msg.id });
    }
    if (msg.send_status === 'sent') {
      entries.push({ ts: msg.sent_at_ms || msg.updated_at_ms || 0, kind: 'sent', label: translate('sent', 'Gesendet'), msgId: msg.id });
    }
    if (msg.send_status === 'cancelled') {
      entries.push({ ts: msg.updated_at_ms || 0, kind: 'cancelled', label: translate('cancelled', 'Abgebrochen'), detail: msg.payload?.cancelled_reason || '', msgId: msg.id });
    }
  }
  for (const approval of approvals) {
    entries.push({
      ts: approval.created_at_ms || 0,
      kind: 'approval',
      label: approval.decision === 'approved' ? translate('approvedBy', 'Freigabe') : translate('rejectedBy', 'Ablehnung'),
      detail: approval.comment || '',
    });
  }
  entries.sort((a, b) => a.ts - b.ts);
  return `
    <ol class="outbound-outreach-timeline">
      ${entries.map((entry) => `
        <li class="outbound-outreach-timeline-item outbound-outreach-timeline-${escapeFn(entry.kind)}">
          <strong>${escapeFn(entry.label)}</strong>
          ${entry.subject ? `<em>${escapeFn(entry.subject)}</em>` : ''}
          ${entry.detail ? `<p>${escapeFn(entry.detail)}</p>` : ''}
          <small>${escapeFn(formatTimestamp(entry.ts))}</small>
        </li>
      `).join('')}
    </ol>
  `;
}

function renderApprovalInbox(campaign) {
  const messages = stateRef.engagementMessages
    .filter((m) => m.campaign_id === campaign.id)
    .filter((m) => (m.approval_status || '') === 'awaiting_approval');
  if (messages.length === 0) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('noApprovalPending', 'Keine Nachrichten warten auf Freigabe.'))}</div>`;
  }
  const slice = messages.slice(0, ACTIVE_OUTREACH_PAGE_SIZE);
  const truncated = messages.length > ACTIVE_OUTREACH_PAGE_SIZE;
  return slice.map((msg) => renderApprovalCard(msg)).join('') +
    (truncated ? `<p class="outbound-outreach-muted">${escapeFn(translate('truncatedList', 'Liste auf {0} eingegrenzt. Insgesamt {1} wartende Nachrichten.', ACTIVE_OUTREACH_PAGE_SIZE, messages.length))}</p>` : '');
}

function renderApprovalCard(msg) {
  const edits = stateRef.activeOutreach.messageEdits.get(msg.id) || {};
  const subject = edits.subject ?? (msg.subject || msg.payload?.subject || '');
  const body = edits.body_text ?? (msg.body_text || msg.payload?.body_text || '');
  const recipient = msg.recipient_email || '';
  const channel = (msg.channel || 'email');
  const isPhysical = channel === 'physical_letter';
  const recipientAddress = msg.recipient_address_text || msg.payload?.recipient_address_text || '';
  const busy = stateRef.activeOutreach.busyMessageIds.has(msg.id);
  const dirty =
    edits.subject !== undefined ||
    edits.body_text !== undefined ||
    edits.recipient_address_text !== undefined ||
    edits.recipient_email !== undefined;
  const messageType = msg.message_type || msg.payload?.message_type || '';
  const replyContext = (messageType === 'reply' || messageType === 'scheduling')
    ? renderReplyContextForMessage(msg)
    : '';
  return `
    <article class="outbound-outreach-approval" data-message-id="${escapeFn(msg.id)}" aria-busy="${busy}">
      <header>
        <div>
          <strong>${escapeFn(msg.payload?.draft_engine || msg.payload?.message_type || 'draft')}</strong>
          <small>${escapeFn(recipient)}</small>
        </div>
        <span class="outbound-outreach-badge outbound-outreach-channel-${escapeFn(channel)}">${escapeFn(isPhysical ? translate('physicalLetter', 'Brief') : translate('email', 'E-Mail'))}</span>
      </header>
      ${replyContext}
      <label>
        <span>${escapeFn(translate('subject', 'Betreff'))}</span>
        <input type="text" value="${escapeFn(subject)}" data-action="ao-edit-subject" data-message-id="${escapeFn(msg.id)}" />
      </label>
      <label>
        <span>${escapeFn(translate('body', 'Text'))}</span>
        <textarea rows="8" data-action="ao-edit-body" data-message-id="${escapeFn(msg.id)}">${escapeFn(body)}</textarea>
      </label>
      ${isPhysical ? `<label><span>${escapeFn(translate('postalAddress', 'Postadresse'))}</span><textarea rows="3" data-action="ao-edit-address" data-message-id="${escapeFn(msg.id)}">${escapeFn(recipientAddress)}</textarea></label>` : ''}
      ${renderProposedSlotsSection(msg)}
      <footer>
        <button type="button" class="outbound-button" data-action="ao-save-draft" data-message-id="${escapeFn(msg.id)}" ${dirty ? '' : 'disabled'}>${escapeFn(translate('saveDraft', 'Draft speichern'))}</button>
        <button type="button" class="outbound-button primary" data-action="ao-approve" data-message-id="${escapeFn(msg.id)}" ${busy ? 'disabled' : ''}>${escapeFn(translate('approve', 'Freigeben'))}</button>
        <button type="button" class="outbound-button" data-action="ao-reject" data-message-id="${escapeFn(msg.id)}" ${busy ? 'disabled' : ''}>${escapeFn(translate('rejectWithComment', 'Ablehnen mit Kommentar'))}</button>
        ${isPhysical ? `<button type="button" class="outbound-button" data-action="ao-print-letter" data-message-id="${escapeFn(msg.id)}">${escapeFn(translate('printLetter', 'Brief PDF/Print'))}</button>` : ''}
      </footer>
      ${busy ? `<div class="outbound-outreach-busy">${escapeFn(translate('processing', 'Wird verarbeitet…'))}</div>` : ''}
    </article>
  `;
}

function renderReadyToSend(campaign) {
  const messages = stateRef.engagementMessages
    .filter((m) => m.campaign_id === campaign.id)
    .filter((m) => (m.approval_status || '') === 'approved')
    .filter((m) => !['sent', 'queued_for_provider'].includes(m.send_status || ''));
  if (messages.length === 0) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('noReadyToSend', 'Keine freigegebenen Nachrichten in der Versand-Queue.'))}</div>`;
  }
  return `
    <ol class="outbound-outreach-ready">
      ${messages.map((msg) => renderReadyToSendItem(msg)).join('')}
    </ol>
  `;
}

function renderReadyToSendItem(msg) {
  const isPhysical = (msg.channel || 'email') === 'physical_letter';
  const recipient = isPhysical
    ? (msg.recipient_address_text || msg.payload?.recipient_address_text || '—')
    : (msg.recipient_email || '—');
  const subject = msg.subject || msg.payload?.subject || '';
  const busy = stateRef.activeOutreach.busyMessageIds.has(msg.id);
  return `
    <li class="outbound-outreach-ready-item">
      <div>
        <strong>${escapeFn(subject)}</strong>
        <small>${escapeFn(recipient)}</small>
      </div>
      <div class="outbound-outreach-actions-cell">
        ${isPhysical
          ? `<button type="button" class="outbound-button primary" data-action="ao-mark-letter-sent" data-message-id="${escapeFn(msg.id)}" ${busy ? 'disabled' : ''}>${escapeFn(translate('markLetterSent', 'Als verschickt markieren'))}</button>`
          : `<button type="button" class="outbound-button primary" data-action="ao-send-approved" data-message-id="${escapeFn(msg.id)}" ${busy ? 'disabled' : ''}>${escapeFn(translate('queueSend', 'In Mailserver-Queue einreihen'))}</button>`}
        <button type="button" class="outbound-button" data-action="ao-cancel-message" data-message-id="${escapeFn(msg.id)}" ${busy ? 'disabled' : ''}>${escapeFn(translate('cancel', 'Abbrechen'))}</button>
      </div>
    </li>
  `;
}

function renderRepliesView(campaign) {
  const engagements = stateRef.engagements
    .filter((e) => e.campaign_id === campaign.id)
    .filter((e) => (e.status || '') === 'reply_received');
  if (engagements.length === 0) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('noRepliesYet', 'Noch keine Antworten zugeordnet.'))}</div>`;
  }
  return `
    <ol class="outbound-outreach-replies">
      ${engagements.map((engagement) => renderReplyCard(engagement)).join('')}
    </ol>
  `;
}

function renderReplyCard(engagement) {
  const replyClass = engagement.payload?.reply_classification || 'unclear';
  const subject = lastMessageForEngagement(engagement.id)?.subject || '';
  return `
    <li class="outbound-outreach-reply">
      <header>
        <div>
          <strong>${escapeFn(engagement.payload?.contact_name || engagement.id)}</strong>
          <small>${escapeFn(engagement.payload?.contact_email || '')}</small>
        </div>
        <span class="outbound-outreach-badge outbound-outreach-reply-${escapeFn(replyClass)}">${escapeFn(prettyReplyClass(replyClass))}</span>
      </header>
      <p>${escapeFn(subject || translate('noLastSubject', '(kein letzter Betreff)'))}</p>
      <div class="outbound-outreach-actions-cell">
        <label>
          <span>${escapeFn(translate('reclassify', 'Neu klassifizieren'))}</span>
          <select data-action="ao-reclassify" data-id="${escapeFn(engagement.id)}">
            ${REPLY_CLASSES.map((cls) => `<option value="${escapeFn(cls)}" ${cls === replyClass ? 'selected' : ''}>${escapeFn(prettyReplyClass(cls))}</option>`).join('')}
          </select>
        </label>
        <button type="button" class="outbound-button primary" data-action="ao-draft-reply" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('prepareReply', 'Antwort vorbereiten'))}</button>
        ${replyClass === 'positive' ? `<button type="button" class="outbound-button" data-action="ao-draft-scheduling" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('prepareScheduling', 'Termin vorbereiten'))}</button>` : ''}
        ${replyClass === 'unsubscribe' ? `<button type="button" class="outbound-button" data-action="ao-add-suppression" data-id="${escapeFn(engagement.id)}">${escapeFn(translate('addSuppression', 'Suppression-Eintrag erstellen'))}</button>` : ''}
      </div>
    </li>
  `;
}

function renderDoneView(campaign) {
  const engagements = stateRef.engagements
    .filter((e) => e.campaign_id === campaign.id)
    .filter((e) => ['closed', 'meeting_booked'].includes(e.status || ''));
  if (engagements.length === 0) {
    return `<div class="outbound-outreach-empty">${escapeFn(translate('noFinishedYet', 'Noch keine abgeschlossenen Engagements.'))}</div>`;
  }
  return `
    <table class="outbound-outreach-table">
      <thead>
        <tr>
          <th>${escapeFn(translate('contact', 'Kontakt'))}</th>
          <th>${escapeFn(translate('status', 'Status'))}</th>
          <th>${escapeFn(translate('reason', 'Grund'))}</th>
          <th>${escapeFn(translate('updated', 'Aktualisiert'))}</th>
        </tr>
      </thead>
      <tbody>
        ${engagements.map((e) => `
          <tr>
            <td>${escapeFn(e.payload?.contact_name || e.id)}</td>
            <td>${escapeFn(prettyStatus(e.status || ''))}</td>
            <td>${escapeFn(e.closed_reason || e.payload?.closed_reason || '—')}</td>
            <td>${escapeFn(formatTimestamp(e.updated_at_ms || 0))}</td>
          </tr>
        `).join('')}
      </tbody>
    </table>
  `;
}

function renderSenderModal(campaign) {
  const modal = stateRef.activeOutreach.sender;
  if (!modal?.open) return '';
  const candidates = senderCandidatesForCampaign(campaign);
  return `
    <div class="outbound-outreach-modal" role="dialog" aria-modal="true" aria-labelledby="ao-sender-modal-title">
      <div class="outbound-outreach-modal-body">
        <h4 id="ao-sender-modal-title">${escapeFn(translate('chooseSender', 'Sender wählen'))}</h4>
        <ul class="outbound-outreach-sender-list">
          ${candidates.length === 0
            ? `<li class="outbound-outreach-empty">${escapeFn(translate('noSenderCandidates', 'Kein Postfach für diese Campaign verknüpft.'))}</li>`
            : candidates.map((cand) => `
              <li class="outbound-outreach-sender-item">
                <button type="button" class="outbound-outreach-sender-pick${cand.healthClass ? ' is-' + cand.healthClass : ''}" data-action="ao-assign-sender" data-engagement-id="${escapeFn(modal.engagementId)}" data-account-key="${escapeFn(cand.account_key)}">
                  <strong>${escapeFn(cand.address)}</strong>
                  <small>${escapeFn(cand.healthLabel)}</small>
                </button>
              </li>
            `).join('')
          }
        </ul>
        <footer>
          <button type="button" class="outbound-button" data-action="ao-close-sender">${escapeFn(translate('cancel', 'Abbrechen'))}</button>
        </footer>
      </div>
    </div>
  `;
}

function senderCandidatesForCampaign(campaign) {
  const key = campaign?.communication_account_key || campaign?.payload?.communication_account_key || '';
  const address = campaign?.communication_account_address || campaign?.payload?.communication_account_address || '';
  if (!key && !address) return [];
  const limit = stateRef.accountLimits.find((l) => (l.id || l.sender_account_id) === key);
  return [{
    account_key: key || `email:${address}`,
    address: address || key.replace(/^email:/, ''),
    healthLabel: senderHealthLabel(limit),
    healthClass: senderHealthClass(limit),
  }];
}

function senderHealthLabel(limit) {
  if (!limit) return translate('senderNotLinked', 'Kein Account-Limit verknüpft');
  if (limit.blocked) return translate('senderBlocked', 'Gesperrt');
  if (['blocked', 'locked', 'suspended', 'disabled'].includes(limit.status)) return translate('senderUnavailable', 'Nicht verfügbar');
  if (typeof limit.daily_limit === 'number' && typeof limit.daily_sent_count === 'number' && limit.daily_limit > 0 && limit.daily_sent_count >= limit.daily_limit) {
    return translate('senderLimit', 'Tageslimit erreicht');
  }
  return translate('senderReady', 'Bereit');
}

function senderHealthClass(limit) {
  if (!limit) return 'warn';
  if (limit.blocked) return 'blocked';
  if (['blocked', 'locked', 'suspended', 'disabled'].includes(limit.status)) return 'blocked';
  if (typeof limit.daily_limit === 'number' && typeof limit.daily_sent_count === 'number' && limit.daily_limit > 0 && limit.daily_sent_count >= limit.daily_limit) return 'limit';
  return 'ready';
}

function computeNextStep(engagement, lastMessage) {
  const status = engagement.status || '';
  if (status === 'reply_received') {
    const replyClass = engagement.payload?.reply_classification || 'unclear';
    if (replyClass === 'positive') {
      return { label: translate('nextStepReplyPositive', 'Positive Antwort – Termin vorbereiten oder Antwort entwerfen'), actions: ['auto_draft_scheduling', 'auto_draft_reply'] };
    }
    return { label: translate('nextStepReply', 'Antwort eingegangen – Antwort vorbereiten'), actions: ['auto_draft_reply'] };
  }
  if (status === 'paused') {
    return { label: translate('nextStepPaused', 'Pausiert – Resume oder Close wählen'), actions: [] };
  }
  if (!lastMessage) {
    return { label: translate('nextStepInitial', 'Initial-Draft vorbereiten'), actions: ['auto_draft_initial'] };
  }
  const approval = lastMessage.approval_status || 'draft';
  const send = lastMessage.send_status || 'draft';
  if (approval === 'awaiting_approval') {
    return { label: translate('nextStepAwaiting', 'Auf Freigabe warten'), actions: ['review_draft'] };
  }
  if (approval === 'approved' && !['sent', 'queued_for_provider'].includes(send)) {
    return { label: translate('nextStepSend', 'Versand starten'), actions: ['review_draft'] };
  }
  if (send === 'sent' || send === 'queued_for_provider') {
    return { label: translate('nextStepFollowup', 'Auf Antwort warten – Follow-up vorbereiten'), actions: ['auto_draft_followup'] };
  }
  if (approval === 'draft' || approval === 'rejected') {
    return { label: translate('nextStepRequestApproval', 'Freigabe anfordern'), actions: ['request_approval'] };
  }
  return { label: translate('nextStepReady', 'Weiter mit Sequenz'), actions: ['auto_draft_followup'] };
}

function renderReplyContextForMessage(msg) {
  const engagement = stateRef.engagements.find((e) => e.id === msg.engagement_id);
  if (!engagement) return '';
  const replyClass = engagement.payload?.reply_classification;
  const replyMessageKey = engagement.payload?.reply_message_id;
  if (!replyClass && !replyMessageKey) return '';
  return `
    <aside class="outbound-outreach-reply-context" role="note">
      <strong>${escapeFn(translate('replyContext', 'Reply-Kontext'))}</strong>
      ${replyClass ? `<span class="outbound-outreach-badge outbound-outreach-reply-${escapeFn(replyClass)}">${escapeFn(prettyReplyClass(replyClass))}</span>` : ''}
      ${replyMessageKey ? `<small>${escapeFn(replyMessageKey)}</small>` : ''}
    </aside>
  `;
}

function renderProposedSlotsSection(msg) {
  const messageType = msg.message_type || msg.payload?.message_type || '';
  const slots = msg.payload?.proposed_slots;
  if (messageType !== 'scheduling' && !Array.isArray(slots)) return '';
  const slotList = Array.isArray(slots) ? slots : [];
  const meetingRequestId = msg.payload?.meeting_request_id || '';
  return `
    <section class="outbound-outreach-slots">
      <header><strong>${escapeFn(translate('proposedSlots', 'Termin-Vorschläge'))}</strong>${meetingRequestId ? `<small>${escapeFn(meetingRequestId)}</small>` : ''}</header>
      ${slotList.length === 0
        ? `<p class="outbound-outreach-muted">${escapeFn(translate('noSlotsYet', 'Noch keine Slots vorgeschlagen.'))}</p>`
        : `<ul>${slotList.map((slot, idx) => {
            const unavailable = slot.available === false;
            return `
            <li class="${unavailable ? 'is-unavailable' : ''}">
              <span>${escapeFn(formatSlotLabel(slot))}${unavailable ? ` <em class="outbound-outreach-slot-busy">${escapeFn(translate('slotBusy', 'belegt'))}</em>` : ''}</span>
              ${meetingRequestId ? `<button type="button" class="outbound-button" data-action="ao-book-slot" data-meeting-request-id="${escapeFn(meetingRequestId)}" data-slot-index="${idx}" ${unavailable ? 'disabled' : ''}>${escapeFn(translate('bookSlot', 'Termin buchen'))}</button>` : ''}
            </li>
          `;
          }).join('')}</ul>`}
      ${meetingRequestId ? `<button type="button" class="outbound-button" data-action="ao-regenerate-slots" data-meeting-request-id="${escapeFn(meetingRequestId)}" data-message-id="${escapeFn(msg.id)}">${escapeFn(translate('regenerateSlots', 'Slots neu generieren'))}</button>` : ''}
    </section>
  `;
}

function formatSlotLabel(slot) {
  if (!slot) return '';
  const startIso = slot.start_iso || slot.startISO || '';
  const endIso = slot.end_iso || slot.endISO || '';
  if (!startIso) return JSON.stringify(slot);
  try {
    const start = new Date(startIso);
    const end = endIso ? new Date(endIso) : null;
    const dateStr = start.toLocaleString(undefined, { weekday: 'short', day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
    const endStr = end ? end.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }) : '';
    return endStr ? `${dateStr} – ${endStr}` : dateStr;
  } catch (_e) {
    return startIso;
  }
}

function renderSkillbookSnapshotSection(lastMessage) {
  const snapshot = lastMessage?.payload?.skillbook_snapshot;
  if (!snapshot || typeof snapshot !== 'object') return '';
  const skillbookId = snapshot.skillbook_id || lastMessage?.payload?.skillbook_id || '';
  const rules = Array.isArray(snapshot.non_negotiable_rules) ? snapshot.non_negotiable_rules : [];
  const workflow = Array.isArray(snapshot.workflow_backbone) ? snapshot.workflow_backbone : [];
  return `
    <section class="outbound-outreach-detail-section outbound-outreach-skillbook">
      <h5>${escapeFn(translate('skillbook', 'Skillbook'))}</h5>
      <p><small>${escapeFn(skillbookId)}</small></p>
      ${snapshot.mission ? `<p>${escapeFn(snapshot.mission)}</p>` : ''}
      ${rules.length ? `<details><summary>${escapeFn(translate('nonNegotiableRules', 'Non-negotiable rules'))} (${rules.length})</summary><ul>${rules.map((rule) => `<li>${escapeFn(rule)}</li>`).join('')}</ul></details>` : ''}
      ${workflow.length ? `<details><summary>${escapeFn(translate('workflowBackbone', 'Workflow'))} (${workflow.length})</summary><ol>${workflow.map((step) => `<li>${escapeFn(step)}</li>`).join('')}</ol></details>` : ''}
    </section>
  `;
}

function detectSequenceVersionMismatch(engagement) {
  const seqId = engagement.sequence_id || engagement.payload?.sequence_id || '';
  if (!seqId) return null;
  const sequence = stateRef.engagementSequences.find((s) => s.id === seqId);
  if (!sequence) return null;
  const latest = sequence.version_number || sequence.payload?.version_number || 1;
  const current = engagement.sequence_version || engagement.payload?.sequence_version || 0;
  if (current && current < latest) {
    return { current, latest };
  }
  return null;
}

function buildConversationsDeepLink(campaign, engagement, lastMessage) {
  if (!engagement) return '';
  const params = new URLSearchParams();
  if (campaign?.id) params.set('campaign_id', campaign.id);
  if (engagement.id) params.set('engagement_id', engagement.id);
  const threadKey = lastMessage?.thread_key || lastMessage?.payload?.thread_key || '';
  if (threadKey) params.set('thread_key', threadKey);
  const messageKey = lastMessage?.communication_message_key || lastMessage?.payload?.communication_message_key || '';
  if (messageKey) params.set('message_key', messageKey);
  if (lastMessage?.id) params.set('outbound_message_id', lastMessage.id);
  const accountKey = lastMessage?.communication_account_key || lastMessage?.payload?.communication_account_key
    || campaign?.communication_account_key || campaign?.payload?.communication_account_key || '';
  if (accountKey) params.set('account_key', accountKey);
  return `#conversations?${params.toString()}`;
}

function lastMessageForEngagement(engagementId) {
  const msgs = stateRef.engagementMessages
    .filter((m) => m.engagement_id === engagementId)
    .sort((a, b) => (a.created_at_ms || 0) - (b.created_at_ms || 0));
  return msgs[msgs.length - 1] || null;
}

function prettyStatus(status) {
  const map = {
    'ready_for_assignment': translate('readyForAssignment', 'Bereit für Sender'),
    'assigned': translate('assigned', 'Sender zugewiesen'),
    'draft_prepared': translate('draftPrepared', 'Draft vorbereitet'),
    'awaiting_approval': translate('awaitingApproval', 'Freigabe ausstehend'),
    'approved_for_send': translate('approvedForSend', 'Freigegeben'),
    'scheduled_to_send': translate('scheduledToSend', 'Im Versand'),
    'paused': translate('paused', 'Pausiert'),
    'reply_received': translate('replyReceived', 'Antwort erhalten'),
    'meeting_booked': translate('meetingBooked', 'Termin gebucht'),
    'closed': translate('closed', 'Geschlossen'),
    'scheduling': translate('scheduling', 'Terminfindung'),
  };
  return map[status] || status;
}

function prettyReplyClass(cls) {
  const map = {
    positive: translate('replyPositive', 'Positiv'),
    negative: translate('replyNegative', 'Negativ'),
    objection: translate('replyObjection', 'Einwand'),
    out_of_office: translate('replyOoo', 'Abwesenheit'),
    referral: translate('replyReferral', 'Weiterleitung'),
    unsubscribe: translate('replyUnsubscribe', 'Abmeldung'),
    unclear: translate('replyUnclear', 'Unklar'),
  };
  return map[cls] || cls;
}

function formatTimestamp(ms) {
  if (!ms) return '';
  try {
    return new Date(ms).toLocaleString();
  } catch (_e) {
    return String(ms);
  }
}

// ------------------ command dispatching ------------------

async function dispatchOutboundCommand(type, recordId, payload, options = {}) {
  if (!stateRef.ctx?.commandBus?.dispatch) {
    throw new Error('RxDB command bus is not available');
  }
  const commandId = `cmd_${type.replaceAll('.', '_')}_${crypto.randomUUID()}`;
  const command = {
    id: commandId,
    module: 'outbound',
    type,
    record_id: recordId || '',
    payload,
    client_context: {
      source_module: 'outbound',
      ...(options.context || {}),
    },
  };
  const result = await stateRef.ctx.commandBus.dispatch(command);
  const acknowledged = await waitForOutboundCommandProjection(result?.command_id || commandId, options.timeoutMs || 45000);
  await projectOutboundCommandResult(acknowledged);
  return acknowledged;
}

async function waitForOutboundCommandProjection(commandId, timeoutMs) {
  const collection = stateRef.ctx?.db?.raw?.business_commands;
  if (!collection || !commandId) {
    throw new Error('business_commands collection is required for outbound command acknowledgements');
  }
  const deadline = Date.now() + timeoutMs;
  let lastStatus = null;
  while (Date.now() < deadline) {
    const doc = await collection.findOne(commandId).exec();
    const command = doc?.toJSON?.() || null;
    lastStatus = command?.status || null;
    if (command && command.status && command.status !== 'pending_sync') {
      if (command.status === 'failed') {
        throw new Error(command.error || `Outbound command ${commandId} failed`);
      }
      return command;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Outbound command ${commandId} was not acknowledged by the native RxDB peer (last status: ${lastStatus || 'missing'}).`);
}

async function projectOutboundCommandResult(command) {
  const result = command?.result;
  const raw = stateRef.ctx?.db?.raw;
  if (!result || !raw) return;
  const projections = [
    ['outbound_engagements', result.engagement],
    ['outbound_messages', result.message],
    ['outbound_approvals', result.approval],
    ['outbound_sender_assignments', result.assignment],
    ['outbound_sequences', result.sequence],
    ['outbound_skillbooks', result.skillbook],
    ['outbound_letter_templates', result.template || result.letter_template],
    ['outbound_meeting_requests', result.meeting_request || result.request],
  ];
  for (const [collectionName, record] of projections) {
    if (!record?.id || !raw[collectionName]) continue;
    await upsertLocalProjection(raw[collectionName], record);
  }
  if (Array.isArray(result.updated)) {
    for (const record of result.updated) {
      const collectionName = record?.collection || record?.collection_name;
      const payload = record?.record || record?.message || record;
      if (!collectionName || !payload?.id || !raw[collectionName]) continue;
      await upsertLocalProjection(raw[collectionName], payload);
    }
  }
}

async function upsertLocalProjection(collection, record) {
  if (collection.incrementalUpsert) {
    await collection.incrementalUpsert(record);
    return;
  }
  if (collection.upsert) {
    await collection.upsert(record);
    return;
  }
  const existing = await collection.findOne(record.id).exec();
  if (existing?.incrementalPatch) {
    await existing.incrementalPatch(record);
    return;
  }
  if (existing?.patch) {
    await existing.patch(record);
    return;
  }
  await collection.insert(record);
}

function setError(msg) {
  stateRef.activeOutreach.error = msg;
  triggerRender();
}

function clearError() {
  if (!stateRef.activeOutreach.error) return;
  stateRef.activeOutreach.error = null;
  triggerRender();
}

function withBusyMessage(messageId, fn) {
  stateRef.activeOutreach.busyMessageIds.add(messageId);
  triggerRender();
  return Promise.resolve()
    .then(fn)
    .catch((error) => {
      setError(error?.message || String(error));
      throw error;
    })
    .finally(() => {
      stateRef.activeOutreach.busyMessageIds.delete(messageId);
      triggerRender();
    });
}

function withBusyEngagement(engagementId, fn) {
  stateRef.activeOutreach.busyEngagementIds.add(engagementId);
  triggerRender();
  return Promise.resolve()
    .then(fn)
    .catch((error) => {
      setError(error?.message || String(error));
      throw error;
    })
    .finally(() => {
      stateRef.activeOutreach.busyEngagementIds.delete(engagementId);
      triggerRender();
    });
}

export async function handleActiveOutreachAction(action, target) {
  clearError();
  const campaign = stateRef.campaigns.find((c) => c.id === stateRef.selectedCampaignId);
  switch (action) {
    case 'ao-view':
      stateRef.activeOutreach.view = target.dataset.view;
      triggerRender();
      return;
    case 'ao-dismiss-error':
      stateRef.activeOutreach.error = null;
      triggerRender();
      return;
    case 'ao-select-engagement':
      stateRef.activeOutreach.selectedEngagementId = target.dataset.id;
      triggerRender();
      return;
    case 'ao-start-engagement':
      return startEngagementFromLead(target.dataset.leadId, campaign, { autoDraft: false });
    case 'ao-auto-draft':
      return startEngagementFromLead(target.dataset.leadId, campaign, { autoDraft: true });
    case 'ao-draft-initial':
      return prepareDraft(target.dataset.id, 'initial');
    case 'ao-draft-followup':
      return prepareDraft(target.dataset.id, 'followup');
    case 'ao-draft-reply':
      return prepareDraft(target.dataset.id, 'reply');
    case 'ao-draft-scheduling':
      return prepareDraft(target.dataset.id, 'scheduling');
    case 'ao-request-approval':
      return requestApproval(target.dataset.messageId);
    case 'ao-approve':
      return approveMessage(target.dataset.messageId);
    case 'ao-reject':
      return rejectMessage(target.dataset.messageId);
    case 'ao-save-draft':
      return saveDraftEdits(target.dataset.messageId);
    case 'ao-send-approved':
      return sendApproved(target.dataset.messageId);
    case 'ao-mark-letter-sent':
      return markLetterSent(target.dataset.messageId);
    case 'ao-cancel-message':
      return cancelMessage(target.dataset.messageId);
    case 'ao-pause-engagement':
      return pauseEngagement(target.dataset.id);
    case 'ao-resume-engagement':
      return resumeEngagement(target.dataset.id);
    case 'ao-close-engagement':
      return closeEngagement(target.dataset.id);
    case 'ao-open-sender':
      stateRef.activeOutreach.sender = { engagementId: target.dataset.id, accountKey: '', open: true };
      triggerRender();
      return;
    case 'ao-close-sender':
      stateRef.activeOutreach.sender = { engagementId: '', accountKey: '', open: false };
      triggerRender();
      return;
    case 'ao-assign-sender':
      return assignSender(target.dataset.engagementId, target.dataset.accountKey);
    case 'ao-reclassify':
      return reclassifyReply(target.dataset.id, target.value);
    case 'ao-view-message':
      stateRef.activeOutreach.view = 'approval_inbox';
      stateRef.activeOutreach.selectedMessageId = target.dataset.messageId;
      triggerRender();
      return;
    case 'ao-print-letter':
      return printPhysicalLetter(target.dataset.messageId);
    case 'ao-add-suppression':
      return addSuppressionFromEngagement(target.dataset.id);
    case 'ao-reapply-sequence':
      return reapplySequence(target.dataset.id);
    case 'ao-reconcile-provider':
      return reconcileProviderStatus();
    case 'ao-book-slot':
      return bookSlot(target.dataset.meetingRequestId, parseInt(target.dataset.slotIndex || '0', 10));
    case 'ao-regenerate-slots':
      return regenerateSlots(target.dataset.meetingRequestId, target.dataset.messageId);
    case 'ao-scheduler-tick':
      return schedulerTick();
    case 'ao-audit-export':
      return auditExport();
    case 'ao-seed-skillbooks':
      return seedSkillbooks();
    case 'ao-save-skillbook':
      return saveSkillbookFromCard(target.dataset.skillbookId);
    case 'ao-new-letter-template':
      return newLetterTemplate(target.dataset.campaignId);
    case 'ao-save-template':
      return saveLetterTemplateFromCard(target.dataset.templateId);
    default:
      return;
  }
}

async function seedSkillbooks() {
  try {
    await dispatchOutboundCommand('outbound.skillbook.seed_defaults', '', {});
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function saveSkillbookFromCard(skillbookId) {
  if (!skillbookId) return;
  const card = stateRef.ctx.host.querySelector(`[data-skillbook-id="${cssEscape(skillbookId)}"]`);
  if (!card) return;
  const mission = card.querySelector('[data-sb-field="mission"]')?.value?.trim() || '';
  const rules = (card.querySelector('[data-sb-field="rules"]')?.value || '')
    .split('\n').map((s) => s.trim()).filter(Boolean);
  const workflow = (card.querySelector('[data-sb-field="workflow"]')?.value || '')
    .split('\n').map((s) => s.trim()).filter(Boolean);
  try {
    await dispatchOutboundCommand('outbound.skillbook.save', skillbookId, {
      skillbook_id: skillbookId,
      mission,
      non_negotiable_rules: rules,
      workflow_backbone: workflow,
    });
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function newLetterTemplate(campaignId) {
  if (!campaignId) return;
  const title = await promptForText(translate('newTemplateTitle', 'Titel der neuen Briefvorlage'));
  if (title === null || title === '') return;
  const templateId = `tpl_${crypto.randomUUID()}`;
  try {
    await dispatchOutboundCommand('outbound.letter_template.save', templateId, {
      template_id: templateId,
      campaign_id: campaignId,
      title,
      salutation: 'Sehr geehrte Damen und Herren,',
      closing: 'Mit freundlichen Grüßen',
      body_template: '',
    });
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function saveLetterTemplateFromCard(templateId) {
  if (!templateId) return;
  const card = stateRef.ctx.host.querySelector(`[data-template-id="${cssEscape(templateId)}"]`);
  if (!card) return;
  const tpl = stateRef.outboundLetterTemplates.find((t) => t.id === templateId);
  const payload = {
    template_id: templateId,
    campaign_id: tpl?.campaign_id || '',
    title: card.querySelector('[data-tpl-field="title"]')?.value?.trim() || '',
    salutation: card.querySelector('[data-tpl-field="salutation"]')?.value || '',
    body_template: card.querySelector('[data-tpl-field="body_template"]')?.value || '',
    closing: card.querySelector('[data-tpl-field="closing"]')?.value || '',
  };
  try {
    await dispatchOutboundCommand('outbound.letter_template.save', templateId, payload);
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

function cssEscape(value) {
  return String(value).replace(/"/g, '\\"');
}

async function auditExport() {
  const campaign = stateRef.campaigns.find((c) => c.id === stateRef.selectedCampaignId);
  if (!campaign) return;
  try {
    const result = await dispatchOutboundCommand('outbound.audit.export', '', {
      campaign_id: campaign.id,
    });
    const exportPayload = result?.result?.export || result?.export || {};
    const json = JSON.stringify({ campaign_id: campaign.id, exported_at: new Date().toISOString(), export: exportPayload }, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `outbound-audit-${campaign.id}-${Date.now()}.json`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function runSchedulerTickQuiet() {
  try {
    const result = await dispatchOutboundCommand('outbound.scheduler.tick', '', {});
    const actions = result?.result?.actions || result?.actions || [];
    if (actions.length > 0) {
      await loadActiveOutreachData();
      triggerRender();
    }
  } catch (error) {
    // Silent — periodic tick should not spam the UI on transient errors.
    console.warn('[outbound] periodic scheduler tick failed', error);
  }
}

async function schedulerTick() {
  try {
    const result = await dispatchOutboundCommand('outbound.scheduler.tick', '', {});
    const actions = result?.result?.actions || result?.actions || [];
    if (actions.length === 0) {
      stateRef.activeOutreach.error = null;
    } else {
      stateRef.activeOutreach.error = translate(
        'schedulerSummary',
        'Scheduler hat {0} Engagements verarbeitet.',
        actions.length,
      );
    }
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function bookSlot(meetingRequestId, slotIndex) {
  if (!meetingRequestId) return;
  const request = stateRef.meetingRequests.find((m) => m.id === meetingRequestId);
  if (!request) return;
  const slots = request.proposed_slots || [];
  const slot = slots[slotIndex];
  if (!slot) {
    setError(translate('noSuchSlot', 'Slot nicht gefunden'));
    return;
  }
  const meetingUrl = await promptForText(translate('meetingUrlPrompt', 'Meeting-URL oder Ort'));
  if (meetingUrl === null) return;
  try {
    await dispatchOutboundCommand('outbound.scheduling.mark_booked', meetingRequestId, {
      meeting_request_id: meetingRequestId,
      meeting_url: meetingUrl,
      booked_at_ms: slot.start_ms || Date.now(),
      booked_slot: slot,
    });
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function regenerateSlots(meetingRequestId, messageId) {
  if (!meetingRequestId) return;
  // Empty proposed_slots forces the backend default generator on next prepare-style call.
  try {
    await dispatchOutboundCommand('outbound.scheduling.update_slots', meetingRequestId, {
      meeting_request_id: meetingRequestId,
      proposed_slots: [],
    });
    if (messageId) {
      const regeneratedMessageId = `msg_${crypto.randomUUID()}`;
      await dispatchOutboundCommand('outbound.draft.prepare', regeneratedMessageId, {
        message_id: regeneratedMessageId,
        engagement_id: stateRef.engagementMessages.find((m) => m.id === messageId)?.engagement_id || '',
        draft_kind: 'scheduling',
        meeting_request_id: meetingRequestId,
        campaign_id: stateRef.engagementMessages.find((m) => m.id === messageId)?.campaign_id || '',
        sender_account_id: stateRef.engagementMessages.find((m) => m.id === messageId)?.sender_account_id || '',
        recipient_email: stateRef.engagementMessages.find((m) => m.id === messageId)?.recipient_email || '',
      }).catch((error) => console.warn('[outbound] regenerate slots draft failed', error));
    }
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function reconcileProviderStatus() {
  try {
    const result = await dispatchOutboundCommand('outbound.provider.reconcile', '', {});
    const updated = result?.result?.updated || result?.updated || [];
    if (updated.length === 0) {
      stateRef.activeOutreach.error = null;
      triggerRender();
      return;
    }
    await loadActiveOutreachData();
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function reapplySequence(engagementId) {
  if (!engagementId) return;
  return withBusyEngagement(engagementId, async () => {
    await dispatchOutboundCommand('outbound.engagement.reapply_sequence', engagementId, {
      engagement_id: engagementId,
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

export function handleActiveOutreachInput(action, target) {
  if (action === 'ao-edit-subject' || action === 'ao-edit-body' || action === 'ao-edit-address') {
    const id = target.dataset.messageId;
    if (!id) return;
    const current = stateRef.activeOutreach.messageEdits.get(id) || {};
    if (action === 'ao-edit-subject') current.subject = target.value;
    if (action === 'ao-edit-body') current.body_text = target.value;
    if (action === 'ao-edit-address') current.recipient_address_text = target.value;
    stateRef.activeOutreach.messageEdits.set(id, current);
    // No re-render on every keystroke; user can press save.
  }
}

async function startEngagementFromLead(leadId, campaign, { autoDraft }) {
  if (!campaign) return;
  const lead = (stateRef.pipeline || []).find((p) => p.id === leadId);
  if (!lead) return;
  const engagementId = `eng_${crypto.randomUUID()}`;
  try {
    await dispatchOutboundCommand('outbound.engagement.create', engagementId, {
      engagement_id: engagementId,
      campaign_id: campaign.id,
      company_id: lead.company_id || lead.payload?.company_id || '',
      contact_id: lead.contact_id || lead.payload?.contact_id || '',
      pipeline_id: lead.id,
      contact_name: lead.contact_name || lead.payload?.contact_name || '',
      contact_email: lead.contact_email || lead.payload?.contact_email || '',
      company_name: lead.company_name || lead.payload?.company_name || '',
      lead_name: lead.lead_name || lead.payload?.lead_name || '',
    });
    const channel = campaign.payload?.active_outreach?.default_channel
      || campaign.channel
      || 'email';
    const mailboxKey = campaign.communication_account_key || campaign.payload?.communication_account_key || '';
    if (channel !== 'physical_letter' && mailboxKey) {
      await dispatchOutboundCommand('outbound.engagement.assign_sender', engagementId, {
        engagement_id: engagementId,
        sender_account_id: mailboxKey,
      });
    }
    if (autoDraft) {
      const draftPayload = {
        engagement_id: engagementId,
        draft_kind: 'initial',
        campaign_id: campaign.id,
        channel,
      };
      if (channel === 'physical_letter') {
        draftPayload.recipient_address_text = lead.contact_address_text
          || lead.payload?.contact_address_text
          || lead.address_text
          || lead.payload?.address_text
          || '';
      } else {
        draftPayload.sender_account_id = mailboxKey;
        draftPayload.recipient_email = lead.contact_email || lead.payload?.contact_email || '';
      }
      const messageId = `msg_${crypto.randomUUID()}`;
      draftPayload.message_id = messageId;
      await dispatchOutboundCommand('outbound.draft.prepare', messageId, draftPayload);
    }
    await loadActiveOutreachData();
    stateRef.activeOutreach.view = autoDraft ? 'approval_inbox' : 'engagements';
    stateRef.activeOutreach.selectedEngagementId = engagementId;
    triggerRender();
  } catch (error) {
    setError(error?.message || String(error));
  }
}

async function prepareDraft(engagementId, draftKind) {
  if (!engagementId || !DRAFT_KINDS.includes(draftKind)) return;
  const engagement = stateRef.engagements.find((e) => e.id === engagementId);
  if (!engagement) return;
  const campaign = stateRef.campaigns.find((c) => c.id === engagement.campaign_id);
  const channel = engagement.payload?.channel
    || campaign?.payload?.active_outreach?.default_channel
    || campaign?.channel
    || 'email';
  const lastMessage = lastMessageForEngagement(engagementId);
  return withBusyEngagement(engagementId, async () => {
    const payload = {
      engagement_id: engagementId,
      draft_kind: draftKind,
      campaign_id: engagement.campaign_id,
      channel,
    };
    if (channel === 'physical_letter') {
      payload.recipient_address_text = engagement.payload?.contact_address_text
        || engagement.payload?.recipient_address_text
        || engagement.recipient_address_text
        || lastMessage?.recipient_address_text
        || lastMessage?.payload?.recipient_address_text
        || '';
    } else {
      payload.sender_account_id = engagement.sender_account_id;
      payload.recipient_email = engagement.payload?.contact_email
        || engagement.contact_email
        || engagement.recipient_email
        || lastMessage?.recipient_email
        || lastMessage?.payload?.recipient_email
        || '';
    }
    if (draftKind === 'scheduling') {
      payload.duration_minutes = engagement.payload?.meeting_duration_minutes || campaign?.payload?.active_outreach?.meeting_duration_minutes || 30;
      payload.slot_hint = translate('defaultSlotHint', 'drei konkrete Zeitfenster in den naechsten Tagen');
      payload.proposed_slots = buildDefaultMeetingSlots(payload.duration_minutes);
    }
    const messageId = `msg_${crypto.randomUUID()}`;
    payload.message_id = messageId;
    await dispatchOutboundCommand('outbound.draft.prepare', messageId, payload);
    await loadActiveOutreachData();
    stateRef.activeOutreach.view = 'approval_inbox';
    triggerRender();
  });
}

function buildDefaultMeetingSlots(durationMinutes = 30) {
  const durationMs = Math.max(15, Number(durationMinutes) || 30) * 60 * 1000;
  const base = new Date();
  base.setSeconds(0, 0);
  const slots = [];
  for (let offset = 1; slots.length < 3 && offset < 10; offset += 1) {
    const day = new Date(base.getTime() + offset * 24 * 60 * 60 * 1000);
    const weekday = day.getDay();
    if (weekday === 0 || weekday === 6) continue;
    const hour = slots.length === 0 ? 10 : slots.length === 1 ? 14 : 11;
    const start = new Date(day);
    start.setHours(hour, slots.length === 2 ? 30 : 0, 0, 0);
    const end = new Date(start.getTime() + durationMs);
    slots.push({
      start_iso: start.toISOString(),
      end_iso: end.toISOString(),
      start_ms: start.getTime(),
      end_ms: end.getTime(),
      available: true,
      source: 'outbound_default_scheduler',
    });
  }
  return slots;
}

async function requestApproval(messageId) {
  if (!messageId) return;
  return withBusyMessage(messageId, async () => {
    await dispatchOutboundCommand('outbound.message.request_approval', messageId, {
      message_id: messageId,
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function approveMessage(messageId) {
  if (!messageId) return;
  return withBusyMessage(messageId, async () => {
    const edits = stateRef.activeOutreach.messageEdits.get(messageId);
    if (edits && Object.keys(edits).length) {
      await dispatchOutboundCommand('outbound.message.update_draft', messageId, {
        message_id: messageId,
        ...edits,
      });
      stateRef.activeOutreach.messageEdits.delete(messageId);
    }
    await dispatchOutboundCommand('outbound.message.approve', messageId, {
      message_id: messageId,
    });
    await loadActiveOutreachData();
    stateRef.activeOutreach.view = 'ready_to_send';
    triggerRender();
  });
}

async function rejectMessage(messageId) {
  if (!messageId) return;
  const comment = await promptForText(translate('rejectComment', 'Grund für die Ablehnung'));
  if (comment === null) return;
  return withBusyMessage(messageId, async () => {
    await dispatchOutboundCommand('outbound.message.reject', messageId, {
      message_id: messageId,
      comment: comment || '',
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function saveDraftEdits(messageId) {
  const edits = stateRef.activeOutreach.messageEdits.get(messageId);
  if (!edits) return;
  return withBusyMessage(messageId, async () => {
    await dispatchOutboundCommand('outbound.message.update_draft', messageId, {
      message_id: messageId,
      ...edits,
    });
    stateRef.activeOutreach.messageEdits.delete(messageId);
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function sendApproved(messageId) {
  if (!messageId) return;
  return withBusyMessage(messageId, async () => {
    await dispatchOutboundCommand('outbound.message.send_approved', messageId, {
      message_id: messageId,
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function markLetterSent(messageId) {
  return sendApproved(messageId);
}

async function cancelMessage(messageId) {
  if (!messageId) return;
  const reason = await promptForText(translate('cancelReason', 'Grund für den Abbruch'));
  if (reason === null) return;
  return withBusyMessage(messageId, async () => {
    await dispatchOutboundCommand('outbound.message.cancel', messageId, {
      message_id: messageId,
      reason: reason || 'manual_cancel',
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function pauseEngagement(engagementId) {
  if (!engagementId) return;
  const reason = await promptForText(translate('pauseReason', 'Pausen-Grund'));
  if (reason === null) return;
  const lastMessage = lastMessageForEngagement(engagementId);
  if (!lastMessage) return;
  return withBusyEngagement(engagementId, async () => {
    await dispatchOutboundCommand('outbound.message.pause', lastMessage.id, {
      message_id: lastMessage.id,
      reason: reason || 'manual_pause',
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function resumeEngagement(engagementId) {
  if (!engagementId) return;
  const lastMessage = lastMessageForEngagement(engagementId);
  return withBusyEngagement(engagementId, async () => {
    if (lastMessage) {
      await dispatchOutboundCommand('outbound.message.resume', lastMessage.id, {
        message_id: lastMessage.id,
      });
    }
    await dispatchOutboundCommand('outbound.engagement.resume', engagementId, {
      engagement_id: engagementId,
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function closeEngagement(engagementId) {
  if (!engagementId) return;
  const reason = await promptForText(translate('closeReason', 'Abschluss-Grund'));
  if (reason === null) return;
  return withBusyEngagement(engagementId, async () => {
    await dispatchOutboundCommand('outbound.engagement.close', engagementId, {
      engagement_id: engagementId,
      reason: reason || 'manual_close',
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function assignSender(engagementId, accountKey) {
  if (!engagementId || !accountKey) return;
  return withBusyEngagement(engagementId, async () => {
    await dispatchOutboundCommand('outbound.engagement.assign_sender', engagementId, {
      engagement_id: engagementId,
      sender_account_id: accountKey,
    });
    stateRef.activeOutreach.sender = { engagementId: '', accountKey: '', open: false };
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function reclassifyReply(engagementId, classification) {
  if (!engagementId || !classification) return;
  return withBusyEngagement(engagementId, async () => {
    await dispatchOutboundCommand('outbound.reply.classify', engagementId, {
      engagement_id: engagementId,
      classification,
    });
    await loadActiveOutreachData();
    triggerRender();
  });
}

async function printPhysicalLetter(messageId) {
  const msg = stateRef.engagementMessages.find((m) => m.id === messageId);
  if (!msg) return;
  const subject = msg.subject || msg.payload?.subject || '';
  const body = msg.body_text || msg.payload?.body_text || '';
  const address = msg.recipient_address_text || msg.payload?.recipient_address_text || '';
  const win = window.open('', '_blank', 'width=720,height=900');
  if (!win) {
    setError(translate('popupBlocked', 'Popup geblockt. Bitte Popups erlauben.'));
    return;
  }
  win.document.write(`
    <html>
    <head><title>${escapeFn(subject || translate('letter', 'Brief'))}</title>
    <style>body{font-family:Helvetica,Arial,sans-serif;padding:32px;color:#111;line-height:1.5;}h1{font-size:18px;}pre{white-space:pre-wrap;font-family:inherit;}</style>
    </head>
    <body>
      <h1>${escapeFn(subject)}</h1>
      <p><strong>${escapeFn(translate('to', 'An'))}:</strong><br />${escapeFn(address).replace(/\n/g, '<br />')}</p>
      <pre>${escapeFn(body)}</pre>
    </body>
    </html>
  `);
  win.document.close();
  setTimeout(() => {
    try { win.print(); } catch (_e) { /* ignore */ }
  }, 250);
}

async function addSuppressionFromEngagement(engagementId) {
  const engagement = stateRef.engagements.find((e) => e.id === engagementId);
  if (!engagement) return;
  const email = engagement.payload?.contact_email || '';
  if (!email) {
    setError(translate('noEmailForSuppression', 'Engagement hat keine Mailadresse.'));
    return;
  }
  const raw = stateRef.ctx?.db?.raw?.outbound_suppression_entries;
  if (!raw) {
    setError(translate('noSuppressionCollection', 'Unterdrückungsliste ist gerade nicht verfügbar.'));
    return;
  }
  const id = `supp_${crypto.randomUUID()}`;
  await raw.insert({
    id,
    email: email.toLowerCase(),
    reason: 'unsubscribe',
    status: 'active',
    source: 'outbound.active_outreach',
    created_at_ms: Date.now(),
    updated_at_ms: Date.now(),
  });
  await dispatchOutboundCommand('outbound.engagement.close', engagementId, {
    engagement_id: engagementId,
    reason: 'unsubscribe',
  });
  await loadActiveOutreachData();
  triggerRender();
}

async function promptForText(label) {
  return new Promise((resolve) => {
    const value = window.prompt(label, '');
    resolve(value === null ? null : (value || ''));
  });
}
