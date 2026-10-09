//! Public V2 assistant items only. Reasoning, tools and legacy unscoped replies
//! never enter this stream. Publication borrows the actual native worker/turn.
use crate::business_os::workjet_supervisor_execution_contract::{
    PublicAssistantText, WireValidate,
};
use anyhow::{ensure, Result};
use ctox_app_server_protocol::{ServerNotification, ThreadItem};
use ctox_protocol::models::MessagePhase;
use std::collections::HashMap;
use std::time::Duration;

const ITEM_CHARS: usize = 65_536;
const TURN_CHARS: usize = 262_144;
const CHUNK_CHARS: usize = 4096;
const MAX_ITEMS: usize = 64;

#[derive(Default)]
struct PublicFilter {
    pending: String,
    hidden: bool,
}
impl PublicFilter {
    fn push(&mut self, text: &str, limit: usize) -> (String, bool) {
        const OPEN: &str = "```ctox-crew";
        let mut public = String::new();
        let mut count = 0;
        for ch in text.chars() {
            self.pending.push(ch);
            loop {
                let marker = if self.hidden { "```" } else { OPEN };
                if self.pending.starts_with(marker) {
                    self.pending.drain(..marker.len());
                    self.hidden = !self.hidden;
                } else if marker.starts_with(&self.pending) {
                    break;
                } else {
                    let first = self.pending.chars().next().expect("nonempty pending");
                    self.pending.drain(..first.len_utf8());
                    if !self.hidden {
                        if count == limit {
                            return (public, true);
                        }
                        public.push(first);
                        count += 1;
                    }
                }
            }
        }
        (public, false)
    }
    fn finish(&mut self) -> String {
        let tail = std::mem::take(&mut self.pending);
        if self.hidden {
            String::new()
        } else {
            tail
        }
    }
}
struct Item {
    phase: String,
    filter: PublicFilter,
    pending: String,
    offset: usize,
    accepted: usize,
    truncated: bool,
    truncation_published: bool,
    completed: bool,
    had_content: bool,
    last_emitted: Duration,
}
#[derive(Default)]
pub(super) struct PublicTextCapture {
    items: HashMap<String, Item>,
    accepted: usize,
}
impl PublicTextCapture {
    pub(super) fn observe(
        &mut self,
        notification: &ServerNotification,
        thread: &str,
        turn: &str,
        elapsed: Duration,
    ) -> Result<Vec<PublicAssistantText>> {
        let (item_id, text, completed) = match notification {
            ServerNotification::ItemStarted(n) if n.thread_id == thread && n.turn_id == turn => {
                let ThreadItem::AgentMessage { id, text, phase } = &n.item else {
                    return Ok(vec![]);
                };
                if self.items.contains_key(id) {
                    return Ok(vec![]);
                }
                ensure!(
                    self.items.len() < MAX_ITEMS,
                    "public assistant item limit exceeded"
                );
                let phase = match phase {
                    Some(MessagePhase::Commentary) => "commentary",
                    Some(MessagePhase::FinalAnswer) => "final_answer",
                    None => "assistant",
                };
                self.items.insert(
                    id.clone(),
                    Item {
                        phase: phase.into(),
                        filter: PublicFilter::default(),
                        pending: String::new(),
                        offset: 0,
                        accepted: 0,
                        truncated: false,
                        truncation_published: false,
                        completed: false,
                        had_content: false,
                        last_emitted: elapsed,
                    },
                );
                (id, text.as_str(), false)
            }
            ServerNotification::AgentMessageDelta(n)
                if n.thread_id == thread && n.turn_id == turn =>
            {
                (&n.item_id, n.delta.as_str(), false)
            }
            ServerNotification::ItemCompleted(n) if n.thread_id == thread && n.turn_id == turn => {
                let ThreadItem::AgentMessage { id, text, .. } = &n.item else {
                    return Ok(vec![]);
                };
                let Some(item) = self.items.get(id) else {
                    return Ok(vec![]);
                };
                // A provider without deltas may deliver its real text at completion.
                // Never replay a completed snapshot over text already streamed.
                (id, if !item.had_content { text.as_str() } else { "" }, true)
            }
            _ => return Ok(vec![]),
        };
        let Some(item) = self.items.get_mut(item_id) else {
            return Ok(vec![]);
        };
        if item.completed {
            return Ok(vec![]);
        }
        item.had_content |= !text.is_empty();
        let allowance = (ITEM_CHARS - item.accepted).min(TURN_CHARS - self.accepted);
        let (mut public, overflow) = if item.truncated {
            (String::new(), false)
        } else {
            item.filter.push(text, allowance)
        };
        if completed && !item.truncated && !overflow {
            public.push_str(&item.filter.finish());
        }
        let count = public.chars().count();
        let accepted = count.min(allowance);
        item.pending.extend(public.chars().take(accepted));
        item.accepted += accepted;
        self.accepted += accepted;
        item.truncated |= overflow || count > accepted;
        let flush = completed
            || item.truncated
            || item.offset == 0
            || item.pending.chars().count() >= 128
            || elapsed.saturating_sub(item.last_emitted) >= Duration::from_millis(100);
        let mut chunks = vec![];
        if flush {
            let pending = std::mem::take(&mut item.pending);
            let chars: Vec<_> = pending.chars().collect();
            for part in chars.chunks(CHUNK_CHARS) {
                chunks.push(PublicAssistantText {
                    turn_id: turn.into(),
                    item_id: item_id.clone(),
                    phase: item.phase.clone(),
                    offset: item.offset as u64,
                    text: part.iter().collect(),
                    completed: false,
                    truncated: false,
                });
                item.offset += part.len();
            }
            item.last_emitted = elapsed;
        }
        if completed || (item.truncated && !item.truncation_published) {
            chunks.push(PublicAssistantText {
                turn_id: turn.into(),
                item_id: item_id.clone(),
                phase: item.phase.clone(),
                offset: item.offset as u64,
                text: String::new(),
                completed,
                truncated: item.truncated,
            });
            item.truncation_published |= item.truncated;
        }
        item.completed = completed;
        for chunk in &chunks {
            chunk.validate().map_err(anyhow::Error::msg)?;
        }
        Ok(chunks)
    }
}

#[cfg(unix)]
pub(super) fn publish(
    root: &std::path::Path,
    provider: &crate::channels::NativeProviderBinding,
    thread: &str,
    turn: &str,
    chunk: &PublicAssistantText,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    chunk.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        provider.runtime_root() == root,
        "public text belongs to another native store"
    );
    ensure!(
        chunk.turn_id == turn,
        "public text belongs to another provider turn"
    );
    provider.with_live_provider_transaction(|tx, facts, actual_turn| {
        ensure!(facts.provider_session_id == thread && actual_turn == Some(turn),
            "public text has no exact live provider turn");
        crate::service::harness_flow::ensure_event_schema(tx)?;
        for (task, attempt) in &facts.routing_attempts {
            let identity = serde_json::to_vec(&(task, &facts.attempt_id, thread, turn,
                &chunk.item_id, chunk.offset, chunk.completed, chunk.truncated))?;
            let id = format!("assistant-text:{:x}", Sha256::digest(identity));
            let metadata = serde_json::json!({ "attempt_id": facts.attempt_id,
                "provider_binding_id": facts.binding_id, "public_text": chunk,
                // These private transcript chunks must not enter the general cockpit.
                "cockpit_eligible": false });
            let json = serde_json::to_string(&metadata)?;
            let existing: Option<String> = {
                use rusqlite::OptionalExtension;
                tx.query_row("SELECT metadata_json FROM ctox_harness_flow_events WHERE event_id=?1",
                    [&id], |row| row.get(0)).optional()?
            };
            if let Some(existing) = existing {
                ensure!(existing == json, "conflicting durable public text chunk");
                continue;
            }
            tx.execute("INSERT INTO ctox_harness_flow_events
                (event_id,chain_key,event_kind,title,body_text,message_key,attempt_index,metadata_json,created_at)
                VALUES (?1,?2,'worker.assistant_text','Assistant response','',?3,?4,?5,?6)",
                rusqlite::params![id, format!("message:{task}"), task, attempt, json,
                    chrono::Utc::now().to_rfc3339()])?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_app_server_protocol::{
        AgentMessageDeltaNotification, ItemCompletedNotification, ItemStartedNotification,
    };
    fn started(phase: Option<MessagePhase>) -> ServerNotification {
        ServerNotification::ItemStarted(ItemStartedNotification {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            item: ThreadItem::AgentMessage {
                id: "item".into(),
                text: String::new(),
                phase,
            },
        })
    }
    fn delta(text: &str) -> ServerNotification {
        ServerNotification::AgentMessageDelta(AgentMessageDeltaNotification {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            item_id: "item".into(),
            delta: text.into(),
        })
    }
    fn completed(text: &str) -> ServerNotification {
        ServerNotification::ItemCompleted(ItemCompletedNotification {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            item: ThreadItem::AgentMessage {
                id: "item".into(),
                text: text.into(),
                phase: Some(MessagePhase::FinalAnswer),
            },
        })
    }
    fn observe(
        capture: &mut PublicTextCapture,
        n: &ServerNotification,
        ms: u64,
    ) -> Vec<PublicAssistantText> {
        capture
            .observe(n, "thread", "turn", Duration::from_millis(ms))
            .unwrap()
    }
    fn text(chunks: &[PublicAssistantText]) -> String {
        chunks.iter().map(|c| c.text.as_str()).collect()
    }
    #[test]
    fn public_text_requires_the_started_item_and_exact_thread_and_turn() {
        let mut c = PublicTextCapture::default();
        assert!(observe(&mut c, &delta("orphan"), 0).is_empty());
        let private = ServerNotification::ItemStarted(ItemStartedNotification {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            item: ThreadItem::Reasoning {
                id: "item".into(),
                summary: vec!["private thinking".into()],
                content: vec![],
            },
        });
        assert!(observe(&mut c, &private, 0).is_empty());
        assert!(observe(&mut c, &delta("private thinking"), 0).is_empty());
        assert!(c
            .observe(&started(None), "foreign", "turn", Duration::ZERO)
            .unwrap()
            .is_empty());
        assert!(c
            .observe(&started(None), "thread", "foreign", Duration::ZERO)
            .unwrap()
            .is_empty());
        observe(&mut c, &started(Some(MessagePhase::Commentary)), 0);
        let chunks = observe(&mut c, &delta("Actual public progress"), 1);
        assert_eq!(text(&chunks), "Actual public progress");
        assert_eq!(chunks[0].phase, "commentary");
        assert_eq!(chunks[0].offset, 0);
        assert!(
            observe(&mut c, &completed("Actual public progress"), 2)
                .last()
                .unwrap()
                .completed
        );
        assert!(observe(&mut c, &delta("late"), 3).is_empty());
    }
    #[test]
    fn public_text_coalesces_tokens_and_flushes_the_real_completion_once() {
        let mut c = PublicTextCapture::default();
        observe(&mut c, &started(Some(MessagePhase::FinalAnswer)), 0);
        assert_eq!(text(&observe(&mut c, &delta("Hallo"), 1)), "Hallo");
        assert!(observe(&mut c, &delta(" Welt"), 2).is_empty());
        let flush = observe(&mut c, &completed("Hallo Welt"), 3);
        assert_eq!(text(&flush), " Welt");
        assert_eq!(flush[0].offset, 5);
        assert!(flush.last().unwrap().completed);
        assert_eq!(flush.last().unwrap().offset, 10);
        assert!(observe(&mut c, &completed("Hallo Welt"), 4).is_empty());
        let mut c = PublicTextCapture::default();
        observe(&mut c, &started(None), 0);
        assert_eq!(
            text(&observe(
                &mut c,
                &completed("Actual non-streaming reply"),
                1
            )),
            "Actual non-streaming reply"
        );
    }
    #[test]
    fn public_text_removes_private_crew_blocks_across_every_token_boundary() {
        let mut c = PublicTextCapture::default();
        observe(&mut c, &started(Some(MessagePhase::FinalAnswer)), 0);
        let raw = "Public\n```ctox-crew\n{\"private\":\"metadata\"}\n```\nEnde";
        let mut all = vec![];
        for (i, ch) in raw.chars().enumerate() {
            all.extend(observe(&mut c, &delta(&ch.to_string()), i as u64 * 101));
        }
        all.extend(observe(&mut c, &completed(raw), 10_000));
        assert_eq!(text(&all), "Public\n\nEnde");
        assert!(!text(&all).contains("private"));
    }
    #[test]
    fn public_text_filter_bounds_auxiliary_memory_for_a_large_provider_chunk() {
        let mut filter = PublicFilter::default();
        let (public, truncated) = filter.push(&"x".repeat(1024 * 1024), 128);
        assert_eq!(public.len(), 128);
        assert!(truncated);
        assert!(filter.pending.len() <= 16);
    }

    #[test]
    fn public_text_unicode_bounds_are_explicit_and_no_snapshot_is_replayed() {
        let mut c = PublicTextCapture::default();
        observe(&mut c, &started(Some(MessagePhase::FinalAnswer)), 0);
        let raw = "🦊".repeat(ITEM_CHARS + 1);
        let chunks = observe(&mut c, &delta(&raw), 1);
        assert_eq!(text(&chunks).chars().count(), ITEM_CHARS);
        assert!(chunks.iter().all(|c| c.text.chars().count() <= CHUNK_CHARS));
        assert!(chunks.last().unwrap().truncated);
        assert_eq!(chunks.last().unwrap().offset, ITEM_CHARS as u64);
        let done = observe(&mut c, &completed(&raw), 2);
        assert_eq!(text(&done), "");
        assert!(done.last().unwrap().completed && done.last().unwrap().truncated);
    }
    #[test]
    fn public_text_publication_is_idempotent_and_rejects_replaced_or_retired_authority(
    ) -> Result<()> {
        let (root, owner, lifetime) = crate::channels::public_text_provider_fixture()?;
        let binding = owner.binding();
        let chunk = PublicAssistantText {
            turn_id: "actual-turn".into(),
            item_id: "item".into(),
            phase: "final_answer".into(),
            offset: 0,
            text: "Actual public text".into(),
            completed: false,
            truncated: false,
        };
        publish(
            root.path(),
            &binding,
            "actual-thread",
            "actual-turn",
            &chunk,
        )?;
        publish(
            root.path(),
            &binding,
            "actual-thread",
            "actual-turn",
            &chunk,
        )?;
        let conn = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
        let count = || {
            conn.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='worker.assistant_text'", [], |r| r.get::<_,i64>(0))
        };
        assert_eq!(count()?, 1);
        assert!(publish(root.path(), &binding, "foreign", "actual-turn", &chunk).is_err());
        assert!(publish(root.path(), &binding, "actual-thread", "foreign", &chunk).is_err());
        let mut conflict = chunk.clone();
        conflict.text = "Conflicting text".into();
        assert!(publish(
            root.path(),
            &binding,
            "actual-thread",
            "actual-turn",
            &conflict
        )
        .is_err());
        lifetime.revoke();
        assert!(publish(
            root.path(),
            &binding,
            "actual-thread",
            "actual-turn",
            &chunk
        )
        .is_err());
        drop(owner);
        assert!(publish(
            root.path(),
            &binding,
            "actual-thread",
            "actual-turn",
            &chunk
        )
        .is_err());
        assert_eq!(count()?, 1);
        Ok(())
    }
}
