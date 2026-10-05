// ref: internal/runtime/executor/claude_executor_tool_state.go:1-139 @ 16d98881d4bb37adaa827599e4be8f5154e81646
// Port-Status: candidate
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

const CLAUDE_OAUTH_TOOL_ALIAS_STATE_LIMIT: usize = 1024;

pub(super) const THREAD_NOT_FOUND_BODY: &[u8] = br#"{"type":"error","error":{"type":"not_found_error","message":"No thread state was found for the requested previous_message_id. Replay the full conversation with thread create to start a new Thread."}}"#;

#[derive(Default)]
struct AliasState {
    entries: HashMap<String, HashMap<String, String>>,
    order: VecDeque<String>,
}

/// Owned by one account executor. An empty known map is distinct from missing
/// state; reads and replacement do not refresh upstream's FIFO insertion order.
#[derive(Default)]
pub(super) struct ClaudeOAuthToolAliasStore {
    state: Mutex<AliasState>,
}

impl ClaudeOAuthToolAliasStore {
    pub(super) fn resolve(&self, payload: &[u8]) -> Result<Option<HashMap<String, String>>, ()> {
        let Ok(document) = std::str::from_utf8(payload) else {
            return Ok(None);
        };
        let previous = gjson::get(document, "thread.previous_message_id");
        if gjson::get(document, "thread.type").str() != "continue" || previous.str().is_empty() {
            return Ok(None);
        }
        let tools = gjson::get(document, "tools");
        if tools.exists() && tools.is_array() && !tools.array().is_empty() {
            return Ok(None);
        }
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state
            .entries
            .get(&format!("message:{}", previous.str()))
            .cloned()
            .map(Some)
            .ok_or(())
    }

    pub(super) fn remember(
        &self,
        keys: Option<&[String]>,
        aliases: &HashMap<String, String>,
        message_id: &str,
    ) {
        let Some(keys) = keys else { return };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for key in keys
            .iter()
            .cloned()
            .chain((!message_id.is_empty()).then(|| format!("message:{message_id}")))
        {
            if key.is_empty() {
                continue;
            }
            if !state.entries.contains_key(&key) {
                state.order.push_back(key.clone());
            }
            state.entries.insert(key, aliases.clone());
        }
        while state.order.len() > CLAUDE_OAUTH_TOOL_ALIAS_STATE_LIMIT {
            if let Some(oldest) = state.order.pop_front() {
                state.entries.remove(&oldest);
            }
        }
    }
}

/// Carry only message keys into the stream owner, never the whole request body.
pub(super) fn thread_alias_keys(payload: &[u8]) -> Option<Vec<String>> {
    let document = std::str::from_utf8(payload).ok()?;
    if gjson::get(document, "thread.type").str().is_empty() {
        return None;
    }
    let previous = gjson::get(document, "thread.previous_message_id");
    Some(if previous.str().is_empty() {
        Vec::new()
    } else {
        vec![format!("message:{}", previous.str())]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn continuation(id: &str) -> Vec<u8> {
        serde_json::to_vec(
            &serde_json::json!({"thread":{"type":"continue","previous_message_id":id}}),
        )
        .unwrap()
    }

    #[test]
    fn candidate_claude_thread_alias_store_predicate_and_known_empty_state() {
        let store = ClaudeOAuthToolAliasStore::default();
        for body in [
            br#"{}"#.as_slice(),
            br#"{"thread":{"type":"create","previous_message_id":"missing"}}"#,
            br#"{"thread":{"type":"continue","previous_message_id":""}}"#,
            br#"{"thread":{"type":"continue","previous_message_id":"missing"},"tools":[{"name":"Read"}]}"#,
            b"invalid",
        ] {
            assert_eq!(store.resolve(body), Ok(None));
        }
        for tools in ["", r#","tools":[]"#, r#","tools":null"#, r#","tools":{}"#] {
            let body = format!(
                r#"{{"thread":{{"type":"continue","previous_message_id":"missing"}}{tools}}}"#
            );
            assert!(store.resolve(body.as_bytes()).is_err());
        }
        let keys = thread_alias_keys(br#"{"thread":{"type":"create"}}"#).unwrap();
        store.remember(Some(&keys), &HashMap::new(), "empty");
        assert_eq!(
            store.resolve(&continuation("empty")),
            Ok(Some(HashMap::new()))
        );
        assert!(thread_alias_keys(br#"{}"#).is_none());
    }

    #[test]
    fn candidate_claude_thread_alias_store_clones_keys_and_isolates_owners() {
        let first = ClaudeOAuthToolAliasStore::default();
        let second = ClaudeOAuthToolAliasStore::default();
        let keys =
            thread_alias_keys(br#"{"thread":{"type":"continue","previous_message_id":"one"}}"#)
                .unwrap();
        let mut aliases = HashMap::from([("alias".to_owned(), "Read".to_owned())]);
        first.remember(Some(&keys), &aliases, "two");
        aliases.clear();
        for id in ["one", "two"] {
            let mut returned = first.resolve(&continuation(id)).unwrap().unwrap();
            assert_eq!(returned["alias"], "Read");
            returned.clear();
            assert_eq!(
                first.resolve(&continuation(id)).unwrap().unwrap()["alias"],
                "Read"
            );
            assert!(second.resolve(&continuation(id)).is_err());
        }
        first.remember(None, &HashMap::new(), "not-a-thread");
        assert!(first.resolve(&continuation("not-a-thread")).is_err());
    }

    #[test]
    fn candidate_claude_thread_alias_store_fifo_replacement_does_not_refresh_order() {
        let store = ClaudeOAuthToolAliasStore::default();
        let keys = Vec::new();
        store.remember(Some(&keys), &HashMap::new(), "first");
        for index in 0..CLAUDE_OAUTH_TOOL_ALIAS_STATE_LIMIT - 1 {
            store.remember(Some(&keys), &HashMap::new(), &format!("other-{index}"));
        }
        assert!(store.resolve(&continuation("first")).is_ok());
        store.remember(
            Some(&keys),
            &HashMap::from([("alias".into(), "Read".into())]),
            "first",
        );
        assert_eq!(
            store.resolve(&continuation("first")).unwrap().unwrap()["alias"],
            "Read"
        );
        store.remember(Some(&keys), &HashMap::new(), "last");
        assert!(store.resolve(&continuation("first")).is_err());
        assert!(store.resolve(&continuation("last")).is_ok());
        let state = store.state.lock().unwrap();
        assert_eq!(state.order.len(), CLAUDE_OAUTH_TOOL_ALIAS_STATE_LIMIT);
        assert_eq!(state.entries.len(), CLAUDE_OAUTH_TOOL_ALIAS_STATE_LIMIT);
    }
}
