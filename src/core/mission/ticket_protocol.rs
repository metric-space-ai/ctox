use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TicketControlNote {
    pub schema: String,
    pub operation: String,
    pub case_id: String,
    pub ticket_key: String,
    pub label: String,
    pub bundle_label: String,
    pub bundle_version: i64,
    pub approval_mode: String,
    pub autonomy_level: String,
    pub support_mode: String,
    pub risk_level: String,
    pub verification_status: Option<String>,
    pub verification_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketCommentWritebackRequest<'a> {
    pub remote_ticket_id: &'a str,
    pub body: &'a str,
    pub internal: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketTransitionWritebackRequest<'a> {
    pub remote_ticket_id: &'a str,
    pub state: &'a str,
    pub note_body: Option<&'a str>,
    pub internal_note: bool,
    pub control_note: Option<TicketControlNote>,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketSelfWorkPublishRequest<'a> {
    pub title: &'a str,
    pub body: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketSelfWorkAssignRequest<'a> {
    pub remote_ticket_id: &'a str,
    pub assignee: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketSelfWorkNoteRequest<'a> {
    pub remote_ticket_id: &'a str,
    pub body: &'a str,
    pub internal: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketSelfWorkTransitionRequest<'a> {
    pub remote_ticket_id: &'a str,
    pub state: &'a str,
    pub note_body: Option<&'a str>,
    pub internal_note: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TicketMirrorRecord {
    pub remote_ticket_id: String,
    pub title: String,
    pub body_text: String,
    pub remote_status: String,
    pub priority: Option<String>,
    pub requester: Option<String>,
    pub metadata: Value,
    pub external_created_at: String,
    pub external_updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TicketEventRecord {
    pub remote_ticket_id: String,
    pub remote_event_id: String,
    pub direction: String,
    pub event_type: String,
    pub summary: String,
    pub body_text: String,
    pub metadata: Value,
    pub external_created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TicketSyncBatch {
    pub system: String,
    pub fetched_ticket_count: usize,
    pub tickets: Vec<TicketMirrorRecord>,
    pub events: Vec<TicketEventRecord>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TicketWritebackResult {
    pub remote_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(crate) struct TicketSelfWorkPublishResult {
    pub remote_ticket_id: Option<String>,
    pub remote_locator: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(crate) struct TicketSelfWorkAssignResult {
    pub remote_assignee: Option<String>,
    pub remote_event_ids: Vec<String>,
}
