// Generated from src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.jour_fixe.v1";

pub(crate) trait WireValidate {
    fn validate(&self) -> Result<(), String>;
}
impl WireValidate for String {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for bool {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for u64 {
    fn validate(&self) -> Result<(), String> {
        if *self > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for i64 {
    fn validate(&self) -> Result<(), String> {
        if self.unsigned_abs() > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for f64 {
    fn validate(&self) -> Result<(), String> {
        if !self.is_finite() {
            return Err("non-finite number".into());
        }
        Ok(())
    }
}
impl<T: WireValidate> WireValidate for Vec<T> {
    fn validate(&self) -> Result<(), String> {
        for item in self {
            item.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum MeetingState {
    #[serde(rename = "planned")]
    Planned,
    #[serde(rename = "preparing")]
    Preparing,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "live")]
    Live,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "failed")]
    Failed,
}
impl WireValidate for MeetingState {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum Speaker {
    #[serde(rename = "owner")]
    Owner,
    #[serde(rename = "supervisor")]
    Supervisor,
}
impl WireValidate for Speaker {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum Modality {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "speech")]
    Speech,
}
impl WireValidate for Modality {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum TodoState {
    #[serde(rename = "proposed")]
    Proposed,
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "superseded")]
    Superseded,
}
impl WireValidate for TodoState {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum Priority {
    #[serde(rename = "P0")]
    P0,
    #[serde(rename = "P1")]
    P1,
    #[serde(rename = "P2")]
    P2,
}
impl WireValidate for Priority {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorRef {
    pub(crate) workjet_thread_id: String,
    pub(crate) ctox_thread_key: String,
}
impl WireValidate for SupervisorRef {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.workjet_thread_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SupervisorRef.workjet_thread_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SupervisorRef.workjet_thread_id violates max_chars".into());
            }
        }
        {
            let value = &self.ctox_thread_key;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SupervisorRef.ctox_thread_key violates min_chars".into());
            }
            if value.chars().count() > 512 {
                return Err("SupervisorRef.ctox_thread_key violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalRef {
    pub(crate) goal_id: String,
    pub(crate) revision: u64,
}
impl WireValidate for GoalRef {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.goal_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("GoalRef.goal_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("GoalRef.goal_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioRef {
    pub(crate) file_id: String,
    pub(crate) sha256: String,
    pub(crate) mime_type: String,
    pub(crate) duration_ms: u64,
    pub(crate) narration_text_sha256: String,
    pub(crate) source_run_id: String,
    pub(crate) model: String,
    pub(crate) format: String,
    pub(crate) synthesis_duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) provenance: Option<AudioProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) generation_id: Option<String>,
}
impl WireValidate for AudioRef {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.file_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AudioRef.file_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AudioRef.file_id violates max_chars".into());
            }
        }
        {
            let value = &self.sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("AudioRef.sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("AudioRef.sha256 violates max_chars".into());
            }
        }
        {
            let value = &self.mime_type;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AudioRef.mime_type violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AudioRef.mime_type violates max_chars".into());
            }
        }
        {
            let value = &self.duration_ms;
            value.validate()?;
        }
        {
            let value = &self.narration_text_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("AudioRef.narration_text_sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("AudioRef.narration_text_sha256 violates max_chars".into());
            }
        }
        {
            let value = &self.source_run_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AudioRef.source_run_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AudioRef.source_run_id violates max_chars".into());
            }
        }
        {
            let value = &self.model;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AudioRef.model violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AudioRef.model violates max_chars".into());
            }
        }
        {
            let value = &self.format;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AudioRef.format violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("AudioRef.format violates max_chars".into());
            }
        }
        {
            let value = &self.synthesis_duration_ms;
            value.validate()?;
        }
        if let Some(value) = &self.provenance {
            value.validate()?;
        }
        if let Some(value) = &self.generation_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AudioRef.generation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AudioRef.generation_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Slide {
    pub(crate) id: String,
    pub(crate) position: u64,
    pub(crate) title: String,
    pub(crate) body_markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) audio: Option<AudioRef>,
    pub(crate) meeting_id: String,
}
impl WireValidate for Slide {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Slide.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Slide.id violates max_chars".into());
            }
        }
        {
            let value = &self.position;
            value.validate()?;
        }
        {
            let value = &self.title;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Slide.title violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("Slide.title violates max_chars".into());
            }
        }
        {
            let value = &self.body_markdown;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Slide.body_markdown violates min_chars".into());
            }
            if value.chars().count() > 16384 {
                return Err("Slide.body_markdown violates max_chars".into());
            }
        }
        if let Some(value) = &self.audio {
            value.validate()?;
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Slide.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Slide.meeting_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Comment {
    pub(crate) id: String,
    pub(crate) slide_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) text: String,
    pub(crate) author_user_id: String,
    pub(crate) created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supervisor_event_id: Option<String>,
    pub(crate) meeting_id: String,
}
impl WireValidate for Comment {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Comment.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Comment.id violates max_chars".into());
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Comment.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Comment.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
        }
        {
            let value = &self.x;
            value.validate()?;
            if *value < 0.0 {
                return Err("Comment.x violates minimum".into());
            }
            if *value > 1.0 {
                return Err("Comment.x violates maximum".into());
            }
        }
        {
            let value = &self.y;
            value.validate()?;
            if *value < 0.0 {
                return Err("Comment.y violates minimum".into());
            }
            if *value > 1.0 {
                return Err("Comment.y violates maximum".into());
            }
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Comment.text violates min_chars".into());
            }
            if value.chars().count() > 4096 {
                return Err("Comment.text violates max_chars".into());
            }
        }
        {
            let value = &self.author_user_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Comment.author_user_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("Comment.author_user_id violates max_chars".into());
            }
        }
        {
            let value = &self.created_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Comment.created_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.supervisor_event_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Comment.supervisor_event_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Comment.supervisor_event_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Comment.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Comment.meeting_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TranscriptTurn {
    pub(crate) id: String,
    pub(crate) sequence: u64,
    pub(crate) speaker: Speaker,
    pub(crate) modality: Modality,
    pub(crate) text: String,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) audio: Option<AudioRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sentence_end_latency_ms: Option<u64>,
    pub(crate) meeting_id: String,
}
impl WireValidate for TranscriptTurn {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptTurn.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TranscriptTurn.id violates max_chars".into());
            }
        }
        {
            let value = &self.sequence;
            value.validate()?;
        }
        {
            let value = &self.speaker;
            value.validate()?;
        }
        {
            let value = &self.modality;
            value.validate()?;
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptTurn.text violates min_chars".into());
            }
            if value.chars().count() > 16384 {
                return Err("TranscriptTurn.text violates max_chars".into());
            }
        }
        {
            let value = &self.started_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("TranscriptTurn.started_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.ended_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("TranscriptTurn.ended_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.source_run_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptTurn.source_run_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TranscriptTurn.source_run_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.audio {
            value.validate()?;
        }
        if let Some(value) = &self.stream_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptTurn.stream_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TranscriptTurn.stream_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.sentence_end_latency_ms {
            value.validate()?;
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptTurn.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TranscriptTurn.meeting_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Todo {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) acceptance: String,
    pub(crate) priority: Priority,
    pub(crate) evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) due_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) owner: Option<String>,
}
impl WireValidate for Todo {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Todo.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Todo.id violates max_chars".into());
            }
        }
        {
            let value = &self.title;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Todo.title violates min_chars".into());
            }
            if value.chars().count() > 512 {
                return Err("Todo.title violates max_chars".into());
            }
        }
        {
            let value = &self.acceptance;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Todo.acceptance violates min_chars".into());
            }
            if value.chars().count() > 4096 {
                return Err("Todo.acceptance violates max_chars".into());
            }
        }
        {
            let value = &self.priority;
            value.validate()?;
        }
        {
            let value = &self.evidence_ids;
            value.validate()?;
            if value.len() > 128 {
                return Err("Todo.evidence_ids violates max_items".into());
            }
        }
        if let Some(value) = &self.due_at_ms {
            value.validate()?;
            if *value < 0 {
                return Err("Todo.due_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.owner {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Todo.owner violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("Todo.owner violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TodoList {
    pub(crate) revision: u64,
    pub(crate) status: TodoState,
    pub(crate) items: Vec<Todo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confirmed_by_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confirmed_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) goal: Option<GoalRef>,
    pub(crate) meeting_id: String,
}
impl WireValidate for TodoList {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.revision;
            value.validate()?;
        }
        {
            let value = &self.status;
            value.validate()?;
        }
        {
            let value = &self.items;
            value.validate()?;
            if value.len() > 100 {
                return Err("TodoList.items violates max_items".into());
            }
        }
        if let Some(value) = &self.confirmed_by_user_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TodoList.confirmed_by_user_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("TodoList.confirmed_by_user_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.confirmed_at_ms {
            value.validate()?;
            if *value < 0 {
                return Err("TodoList.confirmed_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.goal {
            value.validate()?;
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TodoList.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TodoList.meeting_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Meeting {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) supervisor: SupervisorRef,
    pub(crate) scheduled_at_ms: i64,
    pub(crate) prepare_at_ms: i64,
    pub(crate) timezone: String,
    pub(crate) state: MeetingState,
    pub(crate) revision: u64,
    pub(crate) deck_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) previous_goal: Option<GoalRef>,
    pub(crate) slides: Vec<Slide>,
    pub(crate) comments: Vec<Comment>,
    pub(crate) transcript: Vec<TranscriptTurn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) todos: Option<TodoList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}
impl WireValidate for Meeting {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Meeting.id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Meeting.id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Meeting.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Meeting.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.owner_user_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Meeting.owner_user_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("Meeting.owner_user_id violates max_chars".into());
            }
        }
        {
            let value = &self.supervisor;
            value.validate()?;
        }
        {
            let value = &self.scheduled_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Meeting.scheduled_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.prepare_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Meeting.prepare_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.timezone;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Meeting.timezone violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Meeting.timezone violates max_chars".into());
            }
        }
        {
            let value = &self.state;
            value.validate()?;
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
        }
        if let Some(value) = &self.previous_goal {
            value.validate()?;
        }
        {
            let value = &self.slides;
            value.validate()?;
            if value.len() > 100 {
                return Err("Meeting.slides violates max_items".into());
            }
        }
        {
            let value = &self.comments;
            value.validate()?;
            if value.len() > 1000 {
                return Err("Meeting.comments violates max_items".into());
            }
        }
        {
            let value = &self.transcript;
            value.validate()?;
            if value.len() > 10000 {
                return Err("Meeting.transcript violates max_items".into());
            }
        }
        if let Some(value) = &self.todos {
            value.validate()?;
        }
        if let Some(value) = &self.error {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Meeting.error violates min_chars".into());
            }
            if value.chars().count() > 4096 {
                return Err("Meeting.error violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrepareRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
}
impl WireValidate for PrepareRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PrepareRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PrepareRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PrepareRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PrepareRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublishDeckRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) deck_revision: u64,
    pub(crate) slides: Vec<Slide>,
}
impl WireValidate for PublishDeckRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PublishDeckRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PublishDeckRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PublishDeckRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PublishDeckRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
        }
        {
            let value = &self.slides;
            value.validate()?;
            if value.len() < 1 {
                return Err("PublishDeckRequest.slides violates min_items".into());
            }
            if value.len() > 100 {
                return Err("PublishDeckRequest.slides violates max_items".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeetingTransitionRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
}
impl WireValidate for MeetingTransitionRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("MeetingTransitionRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("MeetingTransitionRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("MeetingTransitionRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("MeetingTransitionRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddCommentRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) comment_id: String,
    pub(crate) slide_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) text: String,
}
impl WireValidate for AddCommentRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AddCommentRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AddCommentRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AddCommentRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AddCommentRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.comment_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AddCommentRequest.comment_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AddCommentRequest.comment_id violates max_chars".into());
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AddCommentRequest.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AddCommentRequest.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
        }
        {
            let value = &self.x;
            value.validate()?;
            if *value < 0.0 {
                return Err("AddCommentRequest.x violates minimum".into());
            }
            if *value > 1.0 {
                return Err("AddCommentRequest.x violates maximum".into());
            }
        }
        {
            let value = &self.y;
            value.validate()?;
            if *value < 0.0 {
                return Err("AddCommentRequest.y violates minimum".into());
            }
            if *value > 1.0 {
                return Err("AddCommentRequest.y violates maximum".into());
            }
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AddCommentRequest.text violates min_chars".into());
            }
            if value.chars().count() > 4096 {
                return Err("AddCommentRequest.text violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppendTranscriptRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) turn: TranscriptTurn,
}
impl WireValidate for AppendTranscriptRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AppendTranscriptRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AppendTranscriptRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("AppendTranscriptRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("AppendTranscriptRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.turn;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposeTodosRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) proposal_revision: u64,
    pub(crate) items: Vec<Todo>,
}
impl WireValidate for ProposeTodosRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ProposeTodosRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ProposeTodosRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ProposeTodosRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ProposeTodosRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.proposal_revision;
            value.validate()?;
        }
        {
            let value = &self.items;
            value.validate()?;
            if value.len() > 100 {
                return Err("ProposeTodosRequest.items violates max_items".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfirmTodosRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) proposal_revision: u64,
    pub(crate) expected_goal_revision: u64,
}
impl WireValidate for ConfirmTodosRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfirmTodosRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ConfirmTodosRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfirmTodosRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ConfirmTodosRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.proposal_revision;
            value.validate()?;
        }
        {
            let value = &self.expected_goal_revision;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TranscriptEvent {
    pub(crate) stream_id: String,
    pub(crate) sequence: u64,
    pub(crate) is_final: bool,
    pub(crate) text: String,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sentence_end_latency_ms: Option<u64>,
}
impl WireValidate for TranscriptEvent {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.stream_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptEvent.stream_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("TranscriptEvent.stream_id violates max_chars".into());
            }
        }
        {
            let value = &self.sequence;
            value.validate()?;
        }
        {
            let value = &self.is_final;
            value.validate()?;
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("TranscriptEvent.text violates min_chars".into());
            }
            if value.chars().count() > 16384 {
                return Err("TranscriptEvent.text violates max_chars".into());
            }
        }
        {
            let value = &self.started_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("TranscriptEvent.started_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.ended_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("TranscriptEvent.ended_at_ms violates minimum".into());
            }
        }
        if let Some(value) = &self.sentence_end_latency_ms {
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadMeetingRequest {
    pub(crate) project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) meeting_id: Option<String>,
}
impl WireValidate for ReadMeetingRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadMeetingRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ReadMeetingRequest.project_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.meeting_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadMeetingRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ReadMeetingRequest.meeting_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeetingMutationReceipt {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) project_id: String,
    pub(crate) revision: u64,
    pub(crate) state: MeetingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) changed_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) todos_revision: Option<u64>,
}
impl WireValidate for MeetingMutationReceipt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("MeetingMutationReceipt.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("MeetingMutationReceipt.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("MeetingMutationReceipt.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("MeetingMutationReceipt.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("MeetingMutationReceipt.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("MeetingMutationReceipt.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        {
            let value = &self.state;
            value.validate()?;
        }
        if let Some(value) = &self.changed_id {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("MeetingMutationReceipt.changed_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("MeetingMutationReceipt.changed_id violates max_chars".into());
            }
        }
        if let Some(value) = &self.todos_revision {
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NarrateRequest {
    pub(crate) operation_id: String,
    pub(crate) meeting_id: String,
    pub(crate) slide_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) expected_revision: u64,
    pub(crate) narration_text_sha256: String,
}
impl WireValidate for NarrateRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NarrateRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NarrateRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NarrateRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NarrateRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NarrateRequest.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NarrateRequest.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err("NarrateRequest.deck_revision violates minimum".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.narration_text_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("NarrateRequest.narration_text_sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("NarrateRequest.narration_text_sha256 violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NarrationInput {
    pub(crate) slide_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) expected_revision: u64,
    pub(crate) narration_text_sha256: String,
}
impl WireValidate for NarrationInput {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NarrationInput.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NarrationInput.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err("NarrationInput.deck_revision violates minimum".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.narration_text_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("NarrationInput.narration_text_sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("NarrationInput.narration_text_sha256 violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeNarrationReceipt {
    pub(crate) operation_id: String,
    pub(crate) instance_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) slide_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) owner_user_id: String,
    pub(crate) revision: u64,
    pub(crate) audio: AudioRef,
    pub(crate) persisted_at_ms: i64,
    pub(crate) provenance: AudioProvenance,
    pub(crate) provider_verified: bool,
}
impl WireValidate for NativeNarrationReceipt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NativeNarrationReceipt.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NativeNarrationReceipt.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.instance_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NativeNarrationReceipt.instance_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NativeNarrationReceipt.instance_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NativeNarrationReceipt.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NativeNarrationReceipt.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NativeNarrationReceipt.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NativeNarrationReceipt.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NativeNarrationReceipt.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("NativeNarrationReceipt.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err("NativeNarrationReceipt.deck_revision violates minimum".into());
            }
        }
        {
            let value = &self.owner_user_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("NativeNarrationReceipt.owner_user_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("NativeNarrationReceipt.owner_user_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        {
            let value = &self.audio;
            value.validate()?;
        }
        {
            let value = &self.persisted_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("NativeNarrationReceipt.persisted_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.provenance;
            value.validate()?;
        }
        {
            let value = &self.provider_verified;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum AudioProvenance {
    #[serde(rename = "native_gateway")]
    NativeGateway,
    #[serde(rename = "authenticated_owner_local_audio")]
    AuthenticatedOwnerLocalAudio,
}
impl WireValidate for AudioProvenance {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalNarrationRequest {
    pub(crate) operation_id: String,
    pub(crate) instance_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) slide_id: String,
    pub(crate) file_id: String,
    pub(crate) generation_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) expected_revision: u64,
    pub(crate) audio_sha256: String,
    pub(crate) narration_text_sha256: String,
}
impl WireValidate for LocalNarrationRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.instance_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.instance_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.instance_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.file_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.file_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.file_id violates max_chars".into());
            }
        }
        {
            let value = &self.generation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationRequest.generation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationRequest.generation_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err("LocalNarrationRequest.deck_revision violates minimum".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.audio_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("LocalNarrationRequest.audio_sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("LocalNarrationRequest.audio_sha256 violates max_chars".into());
            }
        }
        {
            let value = &self.narration_text_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err(
                    "LocalNarrationRequest.narration_text_sha256 violates min_chars".into(),
                );
            }
            if value.chars().count() > 64 {
                return Err(
                    "LocalNarrationRequest.narration_text_sha256 violates max_chars".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalNarrationReceipt {
    pub(crate) operation_id: String,
    pub(crate) instance_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) slide_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) owner_user_id: String,
    pub(crate) revision: u64,
    pub(crate) audio: AudioRef,
    pub(crate) persisted_at_ms: i64,
    pub(crate) provenance: AudioProvenance,
    pub(crate) provider_verified: bool,
}
impl WireValidate for LocalNarrationReceipt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationReceipt.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationReceipt.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.instance_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationReceipt.instance_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationReceipt.instance_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationReceipt.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationReceipt.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationReceipt.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationReceipt.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationReceipt.slide_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalNarrationReceipt.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err("LocalNarrationReceipt.deck_revision violates minimum".into());
            }
        }
        {
            let value = &self.owner_user_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalNarrationReceipt.owner_user_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("LocalNarrationReceipt.owner_user_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        {
            let value = &self.audio;
            value.validate()?;
        }
        {
            let value = &self.persisted_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("LocalNarrationReceipt.persisted_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.provenance;
            value.validate()?;
        }
        {
            let value = &self.provider_verified;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum LocalTranscriptProvenance {
    #[serde(rename = "authenticated_owner_local_candidate")]
    AuthenticatedOwnerLocalCandidate,
}
impl WireValidate for LocalTranscriptProvenance {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalTranscriptCandidateRequest {
    pub(crate) operation_id: String,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) expected_revision: u64,
    pub(crate) text: String,
}
impl WireValidate for LocalTranscriptCandidateRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "LocalTranscriptCandidateRequest.operation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "LocalTranscriptCandidateRequest.operation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.request_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateRequest.request_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateRequest.request_id violates max_chars".into());
            }
        }
        {
            let value = &self.instance_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "LocalTranscriptCandidateRequest.instance_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "LocalTranscriptCandidateRequest.instance_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateRequest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "LocalTranscriptCandidateRequest.deck_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.text;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateRequest.text violates min_chars".into());
            }
            if value.chars().count() > 4096 {
                return Err("LocalTranscriptCandidateRequest.text violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalTranscriptCandidateReceipt {
    pub(crate) operation_id: String,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) deck_revision: u64,
    pub(crate) owner_user_id: String,
    pub(crate) turn_id: String,
    pub(crate) sequence: u64,
    pub(crate) revision: u64,
    pub(crate) text_sha256: String,
    pub(crate) persisted_at_ms: i64,
    pub(crate) provenance: LocalTranscriptProvenance,
    pub(crate) provider_verified: bool,
}
impl WireValidate for LocalTranscriptCandidateReceipt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "LocalTranscriptCandidateReceipt.operation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "LocalTranscriptCandidateReceipt.operation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.request_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateReceipt.request_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateReceipt.request_id violates max_chars".into());
            }
        }
        {
            let value = &self.instance_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "LocalTranscriptCandidateReceipt.instance_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "LocalTranscriptCandidateReceipt.instance_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateReceipt.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateReceipt.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateReceipt.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateReceipt.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.deck_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "LocalTranscriptCandidateReceipt.deck_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.owner_user_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "LocalTranscriptCandidateReceipt.owner_user_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 256 {
                return Err(
                    "LocalTranscriptCandidateReceipt.owner_user_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.turn_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("LocalTranscriptCandidateReceipt.turn_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("LocalTranscriptCandidateReceipt.turn_id violates max_chars".into());
            }
        }
        {
            let value = &self.sequence;
            value.validate()?;
            if *value < 1 {
                return Err("LocalTranscriptCandidateReceipt.sequence violates minimum".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("LocalTranscriptCandidateReceipt.revision violates minimum".into());
            }
        }
        {
            let value = &self.text_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err(
                    "LocalTranscriptCandidateReceipt.text_sha256 violates min_chars".into(),
                );
            }
            if value.chars().count() > 64 {
                return Err(
                    "LocalTranscriptCandidateReceipt.text_sha256 violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.persisted_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err(
                    "LocalTranscriptCandidateReceipt.persisted_at_ms violates minimum".into(),
                );
            }
        }
        {
            let value = &self.provenance;
            value.validate()?;
        }
        {
            let value = &self.provider_verified;
            value.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "MeetingState" => serde_json::from_value::<MeetingState>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Speaker" => serde_json::from_value::<Speaker>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Modality" => serde_json::from_value::<Modality>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TodoState" => serde_json::from_value::<TodoState>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Priority" => serde_json::from_value::<Priority>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SupervisorRef" => serde_json::from_value::<SupervisorRef>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "GoalRef" => serde_json::from_value::<GoalRef>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "AudioRef" => serde_json::from_value::<AudioRef>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Slide" => serde_json::from_value::<Slide>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Comment" => serde_json::from_value::<Comment>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TranscriptTurn" => serde_json::from_value::<TranscriptTurn>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Todo" => serde_json::from_value::<Todo>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TodoList" => serde_json::from_value::<TodoList>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Meeting" => serde_json::from_value::<Meeting>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "PrepareRequest" => serde_json::from_value::<PrepareRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "PublishDeckRequest" => serde_json::from_value::<PublishDeckRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "MeetingTransitionRequest" => serde_json::from_value::<MeetingTransitionRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "AddCommentRequest" => serde_json::from_value::<AddCommentRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "AppendTranscriptRequest" => serde_json::from_value::<AppendTranscriptRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ProposeTodosRequest" => serde_json::from_value::<ProposeTodosRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ConfirmTodosRequest" => serde_json::from_value::<ConfirmTodosRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "TranscriptEvent" => serde_json::from_value::<TranscriptEvent>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ReadMeetingRequest" => serde_json::from_value::<ReadMeetingRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "MeetingMutationReceipt" => serde_json::from_value::<MeetingMutationReceipt>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "NarrateRequest" => serde_json::from_value::<NarrateRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "NarrationInput" => serde_json::from_value::<NarrationInput>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "NativeNarrationReceipt" => serde_json::from_value::<NativeNarrationReceipt>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "AudioProvenance" => serde_json::from_value::<AudioProvenance>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "LocalNarrationRequest" => serde_json::from_value::<LocalNarrationRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "LocalNarrationReceipt" => serde_json::from_value::<LocalNarrationReceipt>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "LocalTranscriptProvenance" => serde_json::from_value::<LocalTranscriptProvenance>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "LocalTranscriptCandidateRequest" => {
            serde_json::from_value::<LocalTranscriptCandidateRequest>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "LocalTranscriptCandidateReceipt" => {
            serde_json::from_value::<LocalTranscriptCandidateReceipt>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        _ => Err("unknown contract type".into()),
    }
}
