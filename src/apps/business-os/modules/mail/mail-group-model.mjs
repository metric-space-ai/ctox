export const MAIL_GROUP_SCHEMA = 'ctox.mail-group.v1';
export const MAIL_CONTENT_REVISION_SCHEMA = 'ctox.mail-content-revision.v1';

const GROUP_KINDS = new Set(['outbound-sales', 'newsletter', 'support', 'routing', 'free', 'single']);
const INTAKE_MODES = new Set(['fixed', 'dynamic']);
const CONTENT_MODES = new Set(['word', 'email_visual', 'email_html']);
const TERMINAL_ITEM_STATES = new Set(['done', 'completed', 'routed', 'sent', 'delivered', 'accepted']);
const WORKING_ITEM_STATES = new Set(['working', 'in_progress', 'queued', 'queued_for_provider', 'awaiting_approval', 'approved']);

export const MAIL_GROUP_TEMPLATES = Object.freeze({
  'outbound-sales': Object.freeze({
    id: 'outbound-sales',
    title: 'Outbound Sales',
    kind: 'outbound-sales',
    intakeMode: 'fixed',
    contentMode: 'word',
    objective: 'Alle ausgewählten Kontakte bearbeiten, versenden und eingehende Antworten zuordnen.', // v4-allow: text (Auftragsvorgabe = Daten, kein Hilfetext)
  }),
  newsletter: Object.freeze({
    id: 'newsletter',
    title: 'Newsletter',
    kind: 'newsletter',
    intakeMode: 'fixed',
    contentMode: 'email_visual',
    objective: 'Eine Ausgabe versenden, Zustellfehler prüfen und An- sowie Abmeldungen vollständig verbuchen.', // v4-allow: text (Auftragsvorgabe = Daten, kein Hilfetext)
  }),
  support: Object.freeze({
    id: 'support',
    title: 'Support-Topf',
    kind: 'support',
    intakeMode: 'dynamic',
    contentMode: 'word',
    objective: 'Alle Anfragen beantworten oder mit dokumentierter Übergabe an die zuständige App routen.', // v4-allow: text (Auftragsvorgabe = Daten, kein Hilfetext)
  }),
  routing: Object.freeze({
    id: 'routing',
    title: 'Routing-Topf',
    kind: 'routing',
    intakeMode: 'dynamic',
    contentMode: 'word',
    objective: 'Alle enthaltenen E-Mails einer Ziel-App oder Zuständigkeit zuordnen und den Topf abschließen.', // v4-allow: text (Auftragsvorgabe = Daten, kein Hilfetext)
  }),
  free: Object.freeze({
    id: 'free',
    title: 'Freie Gruppe',
    kind: 'free',
    intakeMode: 'fixed',
    contentMode: 'word',
    objective: 'Die enthaltenen E-Mails als begrenzten Arbeitsauftrag vollständig abarbeiten.', // v4-allow: text (Auftragsvorgabe = Daten, kein Hilfetext)
  }),
  single: Object.freeze({
    id: 'single',
    title: 'Einzel-E-Mail',
    kind: 'single',
    intakeMode: 'fixed',
    contentMode: 'word',
    objective: 'Diese einzelne E-Mail versenden oder bearbeiten und den Auftrag anschließend abschließen.', // v4-allow: text (Auftragsvorgabe = Daten, kein Hilfetext)
  }),
});

export function mailGroupTemplate(templateId = 'free') {
  return MAIL_GROUP_TEMPLATES[templateId] || MAIL_GROUP_TEMPLATES.free;
}

export function buildMailGroupRecord(input = {}, now = Date.now()) {
  const template = mailGroupTemplate(input.templateId);
  const kind = GROUP_KINDS.has(input.kind) ? input.kind : template.kind;
  const intakeMode = INTAKE_MODES.has(input.intakeMode) ? input.intakeMode : template.intakeMode;
  const contentMode = CONTENT_MODES.has(input.contentMode) ? input.contentMode : template.contentMode;
  const name = String(input.name || template.title).trim();
  if (!name) throw new TypeError('Mail group requires a name');
  return {
    id: String(input.id || '').trim(),
    name,
    objective: String(input.objective || template.objective).trim(),
    market: String(input.market || ''),
    status: String(input.status || 'active'),
    owner_id: String(input.ownerId || ''),
    source_count: 0,
    company_count: 0,
    qualified_count: 0,
    pipeline_count: 0,
    payload: {
      ...(input.payload || {}),
      mail_group: {
        schema: MAIL_GROUP_SCHEMA,
        kind,
        intake_mode: intakeMode,
        lifecycle_status: String(input.lifecycleStatus || 'active'),
        account_key: String(input.accountKey || ''),
        content: {
          mode: contentMode,
          active_revision_id: String(input.activeRevisionId || ''),
        },
      },
      mail_group_kind: kind,
      created_in_mail_app: true,
    },
    created_at_ms: Number(input.createdAt || now),
    updated_at_ms: Number(input.updatedAt || now),
  };
}

export function emailGroupDescriptor(record = {}) {
  const group = record.payload?.mail_group || {};
  const template = mailGroupTemplate(group.kind || record.payload?.mail_group_kind || 'free');
  return {
    id: String(record.id || ''),
    name: String(record.name || template.title),
    objective: String(record.objective || template.objective),
    kind: GROUP_KINDS.has(group.kind) ? group.kind : template.kind,
    intakeMode: INTAKE_MODES.has(group.intake_mode) ? group.intake_mode : template.intakeMode,
    lifecycleStatus: String(group.lifecycle_status || record.status || 'active'),
    accountKey: String(group.account_key || ''),
    contentMode: CONTENT_MODES.has(group.content?.mode) ? group.content.mode : template.contentMode,
    activeRevisionId: String(group.content?.active_revision_id || ''),
  };
}

export function mailGroupItemState(message = {}) {
  const explicit = String(message.payload?.mail_group_status || message.processing_status || '').toLowerCase();
  if (TERMINAL_ITEM_STATES.has(explicit)) return 'done';
  if (WORKING_ITEM_STATES.has(explicit)) return 'working';
  if (explicit === 'failed' || explicit === 'open' || explicit === 'blocked') return 'open';

  const sendStatus = String(message.send_status || '').toLowerCase();
  if (sendStatus.includes('fail') || sendStatus.includes('bounce')) return 'open';
  if (TERMINAL_ITEM_STATES.has(sendStatus)) return 'done';
  if (WORKING_ITEM_STATES.has(sendStatus)) return 'working';
  if (String(message.approval_status || '').toLowerCase() === 'awaiting_approval') return 'working';
  return 'open';
}

export function deriveMailGroupProgress(groupId, messages = []) {
  const items = (messages || []).filter((message) => String(message.campaign_id || message.payload?.mail_group_id || '') === String(groupId || ''));
  const states = items.map(mailGroupItemState);
  const done = states.filter((state) => state === 'done').length;
  const working = states.filter((state) => state === 'working').length;
  const total = items.length;
  const open = Math.max(0, total - done - working);
  return {
    total,
    open,
    working,
    done,
    percent: total === 0 ? 0 : (done === total ? 100 : Math.min(99, Math.round((done / total) * 100))),
    complete: total > 0 && done === total,
  };
}

export function buildMailContentRevision(input = {}, now = Date.now()) {
  const editorKind = String(input.editorKind || 'word');
  if (!CONTENT_MODES.has(editorKind)) throw new TypeError(`Unsupported mail content editor: ${editorKind}`);
  const sourceRef = input.sourceRef && typeof input.sourceRef === 'object' ? structuredClone(input.sourceRef) : {};
  return {
    schema: MAIL_CONTENT_REVISION_SCHEMA,
    id: String(input.id || ''),
    group_id: String(input.groupId || ''),
    editor_kind: editorKind,
    source_ref: sourceRef,
    source_sha256: String(input.sourceSha256 || ''),
    compiled_html_ref: input.compiledHtmlRef || null,
    compiled_text_ref: input.compiledTextRef || null,
    compiled_assets: Array.isArray(input.compiledAssets) ? structuredClone(input.compiledAssets) : [],
    diagnostics: Array.isArray(input.diagnostics) ? structuredClone(input.diagnostics) : [],
    compiled_sha256: String(input.compiledSha256 || ''),
    compiler_id: String(input.compilerId || ''),
    merge_schema_version: String(input.mergeSchemaVersion || '1'),
    state: String(input.state || 'draft'),
    created_at_ms: Number(input.createdAt || now),
  };
}

export function appendMailContentRevision(content = {}, revision = {}) {
  if (revision?.schema !== MAIL_CONTENT_REVISION_SCHEMA || !String(revision.id || '')) {
    throw new TypeError('A valid mail content revision is required');
  }
  const previous = Array.isArray(content.revisions) ? content.revisions : [];
  const revisions = previous.filter((item) => String(item?.id || '') !== revision.id);
  revisions.push(structuredClone(revision));
  return {
    ...content,
    active_revision_id: revision.id,
    active_revision: structuredClone(revision),
    revisions,
  };
}

export function messageCanAdoptContentRevision(message = {}) {
  return mailGroupItemState(message) !== 'done';
}

export function applyContentRevisionToPending(messages = [], revision = {}) {
  const revisionId = String(revision.id || '');
  const groupId = String(revision.group_id || '');
  return (messages || []).map((message) => {
    const messageGroupId = String(message.campaign_id || message.payload?.mail_group_id || '');
    if (!groupId || messageGroupId !== groupId) return message;
    if (!messageCanAdoptContentRevision(message)) return message;
    return {
      ...message,
      payload: { ...(message.payload || {}), mail_content_revision_id: revisionId },
    };
  });
}
