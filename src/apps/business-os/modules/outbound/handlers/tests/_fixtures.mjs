// Shared fixtures for the outbound handler tests.
export function engagement(overrides = {}) {
  return {
    id: 'eng_1',
    campaign_id: 'camp_1',
    status: 'assigned',
    sender_account_id: 'email:sender@example.com',
    payload: { contact_email: 'lead@acme.test', contact_name: 'Frau Muster', company_name: 'Acme GmbH' },
    created_at_ms: 1,
    updated_at_ms: 1,
    ...overrides,
  };
}

export function message(overrides = {}) {
  return {
    id: 'msg_1',
    engagement_id: 'eng_1',
    campaign_id: 'camp_1',
    message_type: 'initial',
    direction: 'outbound',
    channel: 'email',
    sender_account_id: 'email:sender@example.com',
    recipient_email: 'lead@acme.test',
    subject: 'Hallo',
    body_text: 'Text',
    draft_status: 'ready_for_review',
    approval_status: 'awaiting_approval',
    send_status: 'awaiting_approval',
    payload: {},
    created_at_ms: 1,
    updated_at_ms: 1,
    ...overrides,
  };
}
