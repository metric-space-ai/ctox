//! Same-turn reply selection for the CTOX adapter, outside the forked harness.
use ctox_protocol::models::MessagePhase;
use ctox_protocol::protocol::AgentMessageEvent;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct DirectSessionReplyCapture {
    selected: Option<String>,
    explicit_final: Option<String>,
    commentary: HashSet<String>,
}

impl DirectSessionReplyCapture {
    // The caller must first enforce thread identity and observe its TurnStarted.
    pub(super) fn observe(&mut self, event: &AgentMessageEvent) {
        if event.phase == Some(MessagePhase::Commentary) {
            self.commentary.insert(event.message.clone());
            // A completion echo must not turn known progress into a final answer.
            self.selected = None;
            self.explicit_final = None;
            return;
        }
        if event.phase == Some(MessagePhase::FinalAnswer) {
            // Only a subsequent explicit final can reclassify the exact text.
            self.commentary.remove(&event.message);
        } else if self.commentary.contains(&event.message) {
            self.selected = None;
            self.explicit_final = None;
            return;
        }
        if is_crew_metadata_only(&event.message) {
            self.selected = Some(self.with_metadata(&event.message));
            return;
        }
        self.explicit_final = if event.phase == Some(MessagePhase::FinalAnswer) {
            Some(crate::crew::public_reply_text(&event.message))
                .filter(|text| !text.trim().is_empty())
        } else {
            // Legacy providers retain last-message behavior, but unknown phase
            // is insufficient evidence for recovering an earlier public answer.
            None
        };
        self.selected = Some(event.message.clone());
    }

    // Called only for our TurnComplete, or the existing end-of-stream fallback.
    pub(super) fn complete(
        self,
        last_agent_message: Option<&str>,
        saw_our_turn_started: bool,
    ) -> Option<String> {
        if let Some(last) = last_agent_message.filter(|text| !text.trim().is_empty()) {
            if self.commentary.contains(last) {
                return None;
            }
            // Without a witnessed start, completion remains authoritative but
            // cannot recover any earlier event text.
            return Some(if saw_our_turn_started && is_crew_metadata_only(last) {
                self.with_metadata(last)
            } else {
                last.to_string()
            });
        }
        if saw_our_turn_started {
            self.selected
        } else {
            None
        }
    }

    fn with_metadata(&self, metadata: &str) -> String {
        match &self.explicit_final {
            Some(answer) => format!("{answer}\n\n{metadata}"),
            None => metadata.to_string(),
        }
    }
}

fn is_crew_metadata_only(text: &str) -> bool {
    text.trim_start().starts_with("```ctox-crew")
        && crate::crew::public_reply_text(text).trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = "```ctox-crew\n{\"crew_retrospective\":{}}\n```";

    fn event(text: &str, phase: Option<MessagePhase>) -> AgentMessageEvent {
        AgentMessageEvent {
            message: text.to_string(),
            phase,
        }
    }

    #[test]
    fn final_answer_survives_separate_metadata_and_completion_once() {
        for phase in [None, Some(MessagePhase::FinalAnswer)] {
            let mut capture = DirectSessionReplyCapture::default();
            capture.observe(&event("25", Some(MessagePhase::FinalAnswer)));
            capture.observe(&event(META, phase));
            let reply = capture.complete(Some(META), true).unwrap();
            assert_eq!(reply, format!("25\n\n{META}"));
            assert_eq!(crate::crew::public_reply_text(&reply), "25");
        }
    }

    #[test]
    fn metadata_completion_can_follow_final_without_legacy_metadata_event() {
        let mut capture = DirectSessionReplyCapture::default();
        capture.observe(&event("25", Some(MessagePhase::FinalAnswer)));
        assert_eq!(
            capture.complete(Some(META), true),
            Some(format!("25\n\n{META}"))
        );
    }

    #[test]
    fn commentary_and_unknown_phase_are_not_recovered_from_metadata() {
        for phase in [None, Some(MessagePhase::Commentary)] {
            let mut capture = DirectSessionReplyCapture::default();
            capture.observe(&event("I am checking", phase));
            capture.observe(&event(META, Some(MessagePhase::FinalAnswer)));
            let reply = capture.complete(Some(META), true).unwrap();
            assert_eq!(reply, META);
            assert!(crate::crew::public_reply_text(&reply).is_empty());
        }
    }

    #[test]
    fn commentary_completion_and_missing_completion_are_not_answers() {
        for completion in [None, Some("I am checking")] {
            let mut capture = DirectSessionReplyCapture::default();
            capture.observe(&event("I am checking", Some(MessagePhase::Commentary)));
            assert_eq!(capture.complete(completion, true), None);
        }
    }

    #[test]
    fn earlier_commentary_completion_echo_is_rejected_after_metadata() {
        let mut capture = DirectSessionReplyCapture::default();
        capture.observe(&event("progress", Some(MessagePhase::Commentary)));
        capture.observe(&event(META, Some(MessagePhase::FinalAnswer)));
        assert_eq!(capture.complete(Some("progress"), true), None);
    }

    #[test]
    fn only_subsequent_explicit_final_reclassifies_commentary_text() {
        for phase in [None, Some(MessagePhase::FinalAnswer)] {
            let is_final = phase == Some(MessagePhase::FinalAnswer);
            for completion in [None, Some("25")] {
                let mut capture = DirectSessionReplyCapture::default();
                capture.observe(&event("25", Some(MessagePhase::Commentary)));
                capture.observe(&event("25", phase.clone()));
                assert_eq!(
                    capture.complete(completion, true),
                    is_final.then(|| "25".to_string())
                );
            }
        }
    }

    #[test]
    fn unrelated_commentary_does_not_block_final_or_legacy_answers() {
        for phase in [None, Some(MessagePhase::FinalAnswer)] {
            for completion in [None, Some("25")] {
                let mut capture = DirectSessionReplyCapture::default();
                capture.observe(&event("progress", Some(MessagePhase::Commentary)));
                capture.observe(&event("25", phase.clone()));
                assert_eq!(capture.complete(completion, true).as_deref(), Some("25"));
            }
        }
    }

    #[test]
    fn metadata_alone_does_not_invent_an_answer() {
        let mut capture = DirectSessionReplyCapture::default();
        capture.observe(&event(META, Some(MessagePhase::FinalAnswer)));
        assert_eq!(capture.complete(Some(META), true).as_deref(), Some(META));
    }

    #[test]
    fn combined_answer_and_ordinary_completion_remain_authoritative() {
        let combined = format!("25\n\n{META}");
        for completion in [combined.as_str(), "replacement answer"] {
            let mut capture = DirectSessionReplyCapture::default();
            capture.observe(&event(&combined, Some(MessagePhase::FinalAnswer)));
            assert_eq!(
                capture.complete(Some(completion), true).as_deref(),
                Some(completion)
            );
        }
    }

    #[test]
    fn legacy_answer_fallback_requires_our_start() {
        for started in [false, true] {
            let mut capture = DirectSessionReplyCapture::default();
            capture.observe(&event("legacy answer", None));
            assert_eq!(
                capture.complete(None, started),
                started.then(|| "legacy answer".to_string())
            );
        }
        assert_eq!(
            DirectSessionReplyCapture::default()
                .complete(Some("completion"), false)
                .as_deref(),
            Some("completion")
        );
    }

    #[test]
    fn missing_start_prevents_metadata_recovery() {
        let mut capture = DirectSessionReplyCapture::default();
        capture.observe(&event("25", Some(MessagePhase::FinalAnswer)));
        assert_eq!(capture.complete(Some(META), false).as_deref(), Some(META));
    }

    #[test]
    fn new_unknown_or_commentary_message_invalidates_older_final_candidate() {
        for phase in [None, Some(MessagePhase::Commentary)] {
            let mut capture = DirectSessionReplyCapture::default();
            capture.observe(&event("old answer", Some(MessagePhase::FinalAnswer)));
            capture.observe(&event("new progress", phase));
            assert_eq!(capture.complete(Some(META), true).as_deref(), Some(META));
        }
    }

    #[test]
    fn metadata_without_completion_preserves_final_and_ordinary_fences_stay_text() {
        let mut capture = DirectSessionReplyCapture::default();
        capture.observe(&event("25", Some(MessagePhase::FinalAnswer)));
        capture.observe(&event(META, None));
        assert_eq!(capture.complete(None, true), Some(format!("25\n\n{META}")));
        assert!(!is_crew_metadata_only("```rust\n25\n```"));
        assert!(!is_crew_metadata_only(""));
    }
}
