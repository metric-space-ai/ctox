// ref: internal/runtime/executor/helps/apply_patch_responses.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Request and response `apply_patch` state for a non-Codex executor.
//!
//! The request normalizer runs before Kimi reorders Responses input. The
//! response state converts function-call events and folded namespace
//! dispatchers. It is uncompiled, and Kimi, xAI, and Meta do not call it yet.

use std::collections::{HashMap, HashSet};

use gjson::Kind;

use crate::internal::client::codex::apply_patch::unwrap_input;
use crate::internal::translator::common::{
    prefer_chat_function_patch_tools, qualify_namespace_tool_name, set_json_i64, set_json_string,
    set_raw_path, ApplyPatchResponsesBridge,
};
use crate::sdk::translator::Format;

pub use crate::internal::translator::common::normalize_apply_patch_responses_request;

/// Executor-owned patch conversion. The bridge is never inferred from the wire format.
pub(crate) struct ApplyPatchResponsesState {
    pub(crate) bridge: ApplyPatchResponsesBridge,
    dispatchers: HashMap<String, String>,
    by_dispatcher_key: HashMap<String, usize>,
    records: Vec<PatchDispatcherCall>,
    upstream: Option<Vec<u8>>,
    event_line: Option<Vec<u8>>,
    active: bool,
    failed: bool,
    closed: bool,
    transport_done: bool,
}

struct PatchDispatcherCall {
    namespace: String,
    events: Vec<Vec<u8>>,
    snapshots: Vec<Vec<u8>>,
    source: String,
    originals: Vec<Vec<u8>>,
    completed: bool,
    ordinary: bool,
    index: i64,
    name: String,
    arguments: String,
}

impl PatchDispatcherCall {
    fn new() -> Self {
        Self {
            namespace: String::new(),
            events: Vec::new(),
            snapshots: Vec::new(),
            source: String::new(),
            originals: Vec::new(),
            completed: false,
            ordinary: false,
            index: -1,
            name: String::new(),
            arguments: String::new(),
        }
    }
}

impl ApplyPatchResponsesState {
    /// `declarations` are the original tool declarations, before request normalization.
    pub(crate) fn new(source: &Format, original: &[u8], declarations: &[u8]) -> Self {
        let declarations = if source == &crate::sdk::translator::openai() {
            prefer_chat_function_patch_tools(original, declarations)
        } else {
            declarations.to_vec()
        };
        let bridge = ApplyPatchResponsesBridge::new(&declarations);
        let active = bridge.active();
        Self {
            bridge,
            dispatchers: HashMap::new(),
            by_dispatcher_key: HashMap::new(),
            records: Vec::new(),
            upstream: None,
            event_line: None,
            active,
            failed: false,
            closed: false,
            transport_done: false,
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Marks only folded namespaces that contain a winning custom patch.
    pub(crate) fn add_dispatcher(&mut self, name: &str, namespace: &str) {
        if self.bridge.namespace_has_custom(namespace) {
            self.dispatchers
                .insert(name.to_owned(), namespace.to_owned());
        }
    }

    /// Retains upstream evidence before namespace restoration.
    /// A wrapper alone never proves dispatcher provenance.
    pub(crate) fn remember_dispatcher_event(&mut self, event: &[u8]) {
        if self.failed || self.closed || self.transport_done || self.dispatchers.is_empty() {
            return;
        }
        self.upstream = Some(event.to_vec());
        self.remember_dispatcher_arguments(event);
    }

    /// Keeps full snapshots on every matched candidate, including unnamed calls.
    pub(crate) fn remember_dispatcher_arguments(&mut self, event: &[u8]) {
        if self.failed || self.closed || self.transport_done || self.dispatchers.is_empty() {
            return;
        }
        if json_str(event, "type") != "response.function_call_arguments.done" {
            return;
        }
        self.upstream = Some(event.to_vec());
        let keys = dispatcher_keys_of(event);
        if self.dispatcher_index(&keys).is_none() {
            self.new_dispatcher_candidate(&keys);
        }
        let mut seen = HashSet::new();
        for key in keys {
            if let Some(index) = self.by_dispatcher_key.get(&key).copied() {
                if seen.insert(index) {
                    self.records[index].snapshots.push(event.to_vec());
                }
            }
        }
    }

    pub(crate) fn transform(&mut self, event: &[u8]) -> (Vec<Vec<u8>>, Option<String>) {
        if self.failed || self.transport_done {
            return (Vec::new(), None);
        }
        if self.active && trim_space(event) == b"[DONE]" {
            return match self.finish() {
                Err(error) => self.fail(&error),
                Ok(()) => {
                    self.transport_done = true;
                    (vec![event.to_vec()], None)
                }
            };
        }
        if self.closed {
            return (Vec::new(), None);
        }
        let original = self.upstream.take().unwrap_or_else(|| event.to_vec());
        let kind = json_str(event, "type");
        let mut event = event.to_vec();
        let mut preceding = Vec::new();
        if matches!(
            kind.as_str(),
            "response.completed" | "response.incomplete" | "response.done"
        ) {
            match self.expand_terminal_items(&mut event, &original) {
                Ok(events) => preceding = events,
                Err(error) => return self.fail(&error),
            }
            if self.unfinished_dispatcher() {
                return self
                    .fail("incomplete apply_patch namespace dispatcher received from upstream");
            }
            for call in &mut self.records {
                if call.namespace.is_empty() && !call.ordinary {
                    preceding.extend(std::mem::take(&mut call.events));
                }
            }
        }
        let expanded = match self.expand_dispatcher(event, &original) {
            Ok(events) => events,
            Err(error) => return self.fail(&error),
        };
        preceding.extend(expanded);
        let mut out = Vec::new();
        for event in preceding {
            let transformed = self.bridge.transform(&event);
            out.extend(transformed.events);
            if let Some(error) = transformed.error {
                self.failed = true;
                self.by_dispatcher_key.clear();
                self.records.clear();
                self.upstream = None;
                return (out, Some(error));
            }
        }
        if matches!(
            kind.as_str(),
            "response.completed" | "response.incomplete" | "response.done" | "response.failed"
        ) {
            self.closed = true;
            self.by_dispatcher_key.clear();
            self.records.clear();
            self.upstream = None;
        }
        (out, None)
    }

    /// Validates source closure independently of completed tool input.
    pub(crate) fn finish(&self) -> Result<(), String> {
        self.bridge.finish()?;
        if self.closed || !self.active {
            return Ok(());
        }
        if self.unfinished_dispatcher() {
            return Err(
                "incomplete apply_patch namespace dispatcher received from upstream".to_owned(),
            );
        }
        Err("incomplete apply_patch source response received from upstream".to_owned())
    }

    /// Preserves SSE framing. A premature `[DONE]` is checked before any success marker.
    pub(crate) fn stream(&mut self, line: &[u8]) -> (Vec<Vec<u8>>, Option<String>) {
        if !self.active() {
            return (vec![line.to_vec()], None);
        }
        if self.failed || self.transport_done {
            return (Vec::new(), None);
        }
        if line.starts_with(b"event:") {
            self.event_line = Some(line.to_vec());
            return (Vec::new(), None);
        }
        if !line.starts_with(b"data:") {
            return (vec![line.to_vec()], None);
        }
        let payload = trim_space(&line[5..]).to_vec();
        let (events, error) = if payload == b"[DONE]" {
            match self.finish() {
                Err(error) => self.fail(&error),
                Ok(()) => {
                    self.transport_done = true;
                    self.by_dispatcher_key.clear();
                    self.records.clear();
                    self.upstream = None;
                    self.event_line = None;
                    return (vec![line.to_vec()], None);
                }
            }
        } else {
            self.transform(&payload)
        };
        if events.len() == 1 && events[0] == payload && error.is_none() {
            let mut out = Vec::new();
            if let Some(event_line) = self.event_line.take() {
                out.push(event_line);
            }
            out.push(line.to_vec());
            return (out, None);
        }
        let event_line = self.event_line.clone();
        let mut out = Vec::new();
        for event in &events {
            if let Some(event_line) = &event_line {
                if event == &payload {
                    out.push(event_line.clone());
                } else {
                    out.push(format!("event: {}", json_str(event, "type")).into_bytes());
                }
            }
            let mut data = b"data: ".to_vec();
            data.extend_from_slice(event);
            data.extend_from_slice(b"\n\n");
            out.push(data);
        }
        self.event_line = None;
        (out, error)
    }

    /// Emits the local failure once on EOF without a validated completion.
    pub(crate) fn finish_stream(&mut self) -> (Vec<Vec<u8>>, Option<String>) {
        if self.failed || self.transport_done {
            return (Vec::new(), None);
        }
        let Err(error) = self.finish() else {
            return (Vec::new(), None);
        };
        let (events, failure) = self.fail(&error);
        let events = events
            .into_iter()
            .map(|event| {
                let mut data = b"data: ".to_vec();
                data.extend_from_slice(&event);
                data.extend_from_slice(b"\n\n");
                data
            })
            .collect();
        (events, failure)
    }

    fn fail(&mut self, error: &str) -> (Vec<Vec<u8>>, Option<String>) {
        if self.failed {
            return (Vec::new(), Some(error.to_owned()));
        }
        self.failed = true;
        self.by_dispatcher_key.clear();
        self.records.clear();
        self.upstream = None;
        let transformed = self.bridge.fail(error);
        (transformed.events, transformed.error)
    }

    fn unfinished_dispatcher(&self) -> bool {
        self.records
            .iter()
            .any(|call| !call.namespace.is_empty() && !call.completed)
    }

    fn dispatcher_index(&self, keys: &[String]) -> Option<usize> {
        let matched: HashSet<usize> = keys
            .iter()
            .filter_map(|key| self.by_dispatcher_key.get(key).copied())
            .collect();
        (0..self.records.len()).find(|index| matched.contains(index))
    }

    fn new_dispatcher_candidate(&mut self, keys: &[String]) -> usize {
        let index = self.records.len();
        self.records.push(PatchDispatcherCall::new());
        for key in keys {
            self.by_dispatcher_key.entry(key.clone()).or_insert(index);
        }
        index
    }

    fn expand_terminal_items(
        &mut self,
        event: &mut Vec<u8>,
        original: &[u8],
    ) -> Result<Vec<Vec<u8>>, String> {
        let current_items = array_items(event, "response.output");
        let original_items = array_items(original, "response.output");
        let mut preceding = Vec::new();
        for (position, item) in current_items.iter().enumerate() {
            let item_text = utf8_text(item);
            let item_id = gjson::get(&item_text, "id").str().to_owned();
            let call_id = gjson::get(&item_text, "call_id").str().to_owned();
            let mut done = set_raw_path(br#"{"type":"response.output_item.done"}"#, "item", item);
            let mut call_index = self.dispatcher_index(&dispatcher_keys_of(&done));
            if call_index.is_none() && item_id.is_empty() && call_id.is_empty() {
                done = set_json_i64(&done, "output_index", position as i64);
                call_index = self.dispatcher_index(&dispatcher_keys_of(&done));
            }
            let Some(call_index) = call_index else {
                continue;
            };
            if self.records[call_index].ordinary {
                continue;
            }
            let output_index = if self.records[call_index].index >= 0 {
                self.records[call_index].index
            } else {
                position as i64
            };
            done = set_json_i64(&done, "output_index", output_index);
            let mut original_done = done.clone();
            let same_identity = |candidate: &[u8]| {
                let text = utf8_text(candidate);
                gjson::get(&text, "id").str() == item_id
                    && gjson::get(&text, "call_id").str() == call_id
            };
            if position < original_items.len() && same_identity(&original_items[position]) {
                original_done = set_raw_path(&done, "item", &original_items[position]);
            } else if !item_id.is_empty() || !call_id.is_empty() {
                for original_item in &original_items {
                    if same_identity(original_item) {
                        original_done = set_raw_path(&done, "item", original_item);
                        break;
                    }
                }
            }
            let expanded = self.expand_dispatcher(done, &original_done)?;
            if let Some(last) = expanded.last() {
                let last_text = utf8_text(last);
                let restored = gjson::get(&last_text, "item").json().to_owned();
                *event = set_raw_path(
                    event,
                    &format!("response.output.{position}"),
                    restored.as_bytes(),
                );
            }
            preceding.extend(expanded);
        }
        Ok(preceding)
    }

    fn expand_dispatcher(
        &mut self,
        mut event: Vec<u8>,
        original: &[u8],
    ) -> Result<Vec<Vec<u8>>, String> {
        let event_text = utf8_text(&event);
        let original_text = utf8_text(original);
        let root = gjson::parse(&event_text);
        let raw = gjson::parse(&original_text);
        let kind = root.get("type").str().to_owned();
        let raw_item_type = raw.get("item.type").str().to_owned();
        let mut name = dispatcher_event_name(&raw);
        let delta = root.get("delta").str().to_owned();
        let output_index_exists = root.get("output_index").exists();
        let output_index = root.get("output_index").i64();
        let keys = dispatcher_keys(&root);
        let namespace = self.dispatchers.get(&name).cloned();
        let declared = namespace.is_some();
        let added = kind == "response.output_item.added" && raw_item_type == "function_call";
        let mut call_index = self.dispatcher_index(&keys);
        if call_index.is_none()
            && !self.dispatchers.is_empty()
            && ((declared && raw_item_type == "function_call")
                || added
                || kind == "response.function_call_arguments.delta"
                || kind == "response.function_call_arguments.done")
        {
            call_index = Some(self.new_dispatcher_candidate(&keys));
        }
        let Some(call_index) = call_index else {
            return Ok(vec![event]);
        };
        self.bridge.check_identity(&event)?;
        for key in keys {
            self.by_dispatcher_key.entry(key).or_insert(call_index);
        }
        if self.records[call_index].index < 0 && output_index_exists {
            self.records[call_index].index = output_index;
        }
        if self.records[call_index].ordinary && !declared {
            return Ok(vec![event]);
        }
        self.records[call_index].events.push(event.clone());
        self.records[call_index].originals.push(original.to_vec());
        if let Some(namespace) = namespace {
            if !self.records[call_index].namespace.is_empty()
                && self.records[call_index].namespace != namespace
            {
                return Err("conflicting apply_patch dispatcher namespace".to_owned());
            }
            self.records[call_index].namespace.clone_from(&namespace);
            self.records[call_index].ordinary = false;
        }
        if kind == "response.function_call_arguments.delta" {
            if self.records[call_index].completed && !delta.is_empty() {
                let call_name = self.records[call_index].name.clone();
                let call_namespace = self.records[call_index].namespace.clone();
                let (_, custom) = self.bridge.child_tool(&call_namespace, &call_name);
                if custom {
                    return Err(
                        "apply_patch dispatcher arguments received after completion".to_owned()
                    );
                }
                return Ok(vec![event]);
            }
            self.records[call_index].source.push_str(&delta);
        }
        if self.records[call_index].namespace.is_empty() {
            if !name.is_empty() {
                self.records[call_index].ordinary = true;
                let events = std::mem::take(&mut self.records[call_index].events);
                self.records[call_index].originals.clear();
                return Ok(events);
            }
            return Ok(Vec::new());
        }
        if kind == "response.function_call_arguments.delta" && !self.records[call_index].completed {
            return Ok(Vec::new());
        }
        let reopen = self.records[call_index].completed
            && matches!(
                kind.as_str(),
                "response.function_call_arguments.done"
                    | "response.function_call_arguments.delta"
                    | "response.output_item.added"
            );
        if kind != "response.output_item.done" && !reopen {
            return Ok(Vec::new());
        }
        let path = if matches!(
            kind.as_str(),
            "response.function_call_arguments.done" | "response.function_call_arguments.delta"
        ) {
            ""
        } else {
            "item."
        };
        let call_namespace = self.records[call_index].namespace.clone();
        let call_name = self.records[call_index].name.clone();
        let call_arguments = self.records[call_index].arguments.clone();
        let source_text = self.records[call_index].source.clone();
        let snapshots = self.records[call_index].snapshots.clone();
        let originals = self.records[call_index].originals.clone();
        let stored_events = self.records[call_index].events.clone();
        let mut wrappers = Vec::new();
        if !source_text.is_empty() {
            wrappers.push(source_text);
        }
        for snapshot in &snapshots {
            let arguments = json_str(snapshot, "arguments");
            if gjson::parse(&arguments).get("name").exists() {
                wrappers.push(arguments);
            }
        }
        for pending in &originals {
            let text = utf8_text(pending);
            let parsed = gjson::parse(&text);
            let event_name = dispatcher_event_name(&parsed);
            let arguments = parsed.get("item.arguments").str().to_owned();
            let same_namespace =
                event_name.is_empty() || self.dispatchers.get(&event_name) == Some(&call_namespace);
            if same_namespace && gjson::parse(&arguments).get("name").exists() {
                wrappers.push(arguments);
            }
        }
        if wrappers.is_empty() {
            for pending in &stored_events {
                let arguments = json_str(pending, "arguments");
                if !gjson::parse(&arguments).get("name").str().is_empty() {
                    wrappers.push(arguments);
                }
            }
        }
        let mut selected = String::new();
        for wrapper in &wrappers {
            if json_valid(wrapper) && !gjson::parse(wrapper).get("name").str().is_empty() {
                selected.clone_from(wrapper);
            }
        }
        let source = gjson::parse(&selected);
        name = gjson::parse(&event_text)
            .get(&field_path(path, "name"))
            .str()
            .to_owned();
        if name.is_empty() || self.dispatchers.get(&name) == Some(&call_namespace) {
            name = source.get("name").str().to_owned();
            if name.is_empty() {
                name = call_name;
            }
            event = set_json_string(&event, &field_path(path, "name"), &name);
            event = set_json_string(&event, &field_path(path, "namespace"), &call_namespace);
        }
        if gjson::parse(&event_text)
            .get(&field_path(path, "namespace"))
            .str()
            .is_empty()
        {
            event = set_json_string(&event, &field_path(path, "namespace"), &call_namespace);
        }
        let raw_arguments = raw.get(&field_path(path, "arguments")).str().to_owned();
        if declared
            || dispatcher_event_name(&raw).is_empty()
            || kind == "response.function_call_arguments.done"
        {
            let wrapper = gjson::parse(&raw_arguments);
            if !wrapper.get("name").str().is_empty() {
                event = set_json_string(
                    &event,
                    &field_path(path, "arguments"),
                    &argument_payload(&wrapper.get("arguments")),
                );
            }
        }
        if !gjson::parse(&event_text)
            .get(&field_path(path, "arguments"))
            .exists()
        {
            let mut encoded = argument_payload(&source.get("arguments"));
            if encoded.is_empty() {
                encoded = call_arguments;
            }
            if !encoded.is_empty() {
                event = set_json_string(&event, &field_path(path, "arguments"), &encoded);
            }
        }
        let updated = utf8_text(&event);
        let mut final_arguments = gjson::parse(&updated)
            .get(&field_path(path, "arguments"))
            .str()
            .to_owned();
        if kind == "response.output_item.added" && final_arguments.is_empty() {
            final_arguments.clone_from(&self.records[call_index].arguments);
        }
        if let Some(last) = self.records[call_index].events.last_mut() {
            *last = event.clone();
        }
        let (qualified_name, patch) = self.bridge.child_tool(&call_namespace, &name);
        if patch {
            for snapshot in &snapshots {
                if gjson::get(&utf8_text(snapshot), "arguments").kind() != Kind::String {
                    return Err(
                        "apply_patch dispatcher arguments snapshot must be a string".to_owned()
                    );
                }
            }
        }
        // A custom child remains conflicting evidence when the selected name is ordinary.
        for wrapper_raw in &wrappers {
            let wrapper = gjson::parse(wrapper_raw);
            let wrapper_name = wrapper.get("name").str().to_owned();
            let (_, child_custom) = self.bridge.child_tool(&call_namespace, &wrapper_name);
            if !patch && !child_custom {
                continue;
            }
            let arguments = wrapper.get("arguments");
            let decoded = unwrap_input(&argument_payload(&arguments));
            let final_decoded = unwrap_input(&final_arguments);
            let same = json_valid(wrapper_raw)
                && wrapper_name == name
                && decoded.is_ok()
                && decoded.as_ref().ok() == final_decoded.as_ref().ok();
            if !same {
                return Err("conflicting apply_patch dispatcher arguments".to_owned());
            }
        }
        let start = if self.records[call_index].completed {
            self.records[call_index].events.len().saturating_sub(1)
        } else {
            0
        };
        let replay_events = self.records[call_index].events.clone();
        let replay_originals = self.records[call_index].originals.clone();
        let mut out = Vec::new();
        for index in start..replay_events.len() {
            let mut pending = replay_events[index].clone();
            let pending_text = utf8_text(&pending);
            let pending_root = gjson::parse(&pending_text);
            let pending_type = pending_root.get("type").str().to_owned();
            let original_text = replay_originals
                .get(index)
                .map(|value| utf8_text(value))
                .unwrap_or_default();
            let original_root = gjson::parse(&original_text);
            if patch {
                for namespace_path in ["namespace", "item.namespace"] {
                    let supplied_field = original_root.get(namespace_path);
                    let supplied = supplied_field.str();
                    if !supplied.is_empty() && supplied != call_namespace {
                        return Err("conflicting apply_patch dispatcher namespace".to_owned());
                    }
                }
                for supplied_name in [
                    dispatcher_event_name(&original_root),
                    dispatcher_event_name(&pending_root),
                ] {
                    if !supplied_name.is_empty()
                        && self.dispatchers.get(&supplied_name) != Some(&call_namespace)
                        && qualify_namespace_tool_name(&call_namespace, &supplied_name)
                            != qualified_name
                    {
                        return Err("conflicting apply_patch dispatcher child".to_owned());
                    }
                }
            }
            if pending_type == "response.function_call_arguments.delta" {
                if patch {
                    continue;
                }
                out.push(pending);
                continue;
            }
            let pending_path = match pending_type.as_str() {
                "response.output_item.added" | "response.output_item.done" => "item.",
                "response.function_call_arguments.done" => "",
                _ => {
                    out.push(pending);
                    continue;
                }
            };
            let pending_name = pending_root
                .get(&field_path(pending_path, "name"))
                .str()
                .to_owned();
            if pending_name.is_empty()
                || self.dispatchers.get(&pending_name) == Some(&call_namespace)
            {
                pending = set_json_string(&pending, &field_path(pending_path, "name"), &name);
                pending = set_json_string(
                    &pending,
                    &field_path(pending_path, "namespace"),
                    &call_namespace,
                );
            }
            if pending_root
                .get(&field_path(pending_path, "namespace"))
                .str()
                .is_empty()
            {
                pending = set_json_string(
                    &pending,
                    &field_path(pending_path, "namespace"),
                    &call_namespace,
                );
            }
            let arguments_path = field_path(pending_path, "arguments");
            let arguments = original_root.get(&arguments_path);
            if patch && arguments.exists() && arguments.kind() != Kind::String {
                return Err("apply_patch dispatcher arguments snapshot must be a string".to_owned());
            }
            let original_name = dispatcher_event_name(&original_root);
            if !arguments.str().is_empty()
                && (original_name.is_empty()
                    || self.dispatchers.get(&original_name) == Some(&call_namespace)
                    || pending_path.is_empty())
            {
                let wrapper = gjson::parse(arguments.str());
                if !wrapper.get("name").str().is_empty() {
                    if patch && wrapper.get("name").str() != name {
                        return Err("conflicting apply_patch dispatcher snapshot".to_owned());
                    }
                    pending = set_json_string(
                        &pending,
                        &field_path(pending_path, "arguments"),
                        &argument_payload(&wrapper.get("arguments")),
                    );
                }
            }
            out.push(pending);
        }
        self.records[call_index].completed = true;
        self.records[call_index].name = name;
        self.records[call_index].arguments = final_arguments;
        Ok(out)
    }
}

fn field_path(prefix: &str, field: &str) -> String {
    format!("{prefix}{field}")
}

fn utf8_text(data: &[u8]) -> String {
    String::from_utf8(data.to_vec()).unwrap_or_default()
}

fn json_str(data: &[u8], path: &str) -> String {
    let text = utf8_text(data);
    gjson::get(&text, path).str().to_owned()
}

fn json_valid(value: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(value).is_ok()
}

fn trim_space(data: &[u8]) -> &[u8] {
    let space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let start = data
        .iter()
        .position(|byte| !space(*byte))
        .unwrap_or(data.len());
    let end = data
        .iter()
        .rposition(|byte| !space(*byte))
        .map(|index| index + 1)
        .unwrap_or(start);
    &data[start..end]
}

fn dispatcher_event_name(root: &gjson::Value<'_>) -> String {
    if root.get("item").exists() {
        root.get("item.name").str().to_owned()
    } else {
        root.get("name").str().to_owned()
    }
}

fn dispatcher_keys(root: &gjson::Value<'_>) -> Vec<String> {
    let mut keys = Vec::new();
    for path in ["item.id", "item_id"] {
        let id_field = root.get(path);
        let id = id_field.str();
        if !id.is_empty() {
            keys.push(format!("item:{id}"));
        }
    }
    for path in ["item.call_id", "call_id"] {
        let id_field = root.get(path);
        let id = id_field.str();
        if !id.is_empty() {
            keys.push(format!("call:{id}"));
        }
    }
    let index = root.get("output_index");
    if index.exists() {
        keys.push(format!("index:{}", index.i64()));
    }
    keys
}

fn dispatcher_keys_of(event: &[u8]) -> Vec<String> {
    let text = utf8_text(event);
    dispatcher_keys(&gjson::parse(&text))
}

fn array_items(data: &[u8], path: &str) -> Vec<Vec<u8>> {
    let text = utf8_text(data);
    gjson::parse(&text)
        .get(path)
        .array()
        .iter()
        .map(|item| item.json().as_bytes().to_vec())
        .collect()
}

fn argument_payload(value: &gjson::Value<'_>) -> String {
    if value.kind() == Kind::String {
        value.str().to_owned()
    } else {
        value.json().to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::ApplyPatchResponsesState;
    use crate::sdk::translator::{codex, openai, openai_response};

    fn field(event: &[u8], path: &str) -> String {
        let text = std::str::from_utf8(event).unwrap_or("");
        gjson::get(text, path).str().to_owned()
    }

    fn joined(events: &[Vec<u8>]) -> Vec<u8> {
        events.iter().flatten().copied().collect()
    }

    fn assert_ok(result: (Vec<Vec<u8>>, Option<String>)) -> Vec<Vec<u8>> {
        assert!(result.1.is_none(), "{:?} {:?}", result.1, result.0);
        result.0
    }

    #[test]
    fn native_sse_bytes_pass_through() {
        let request = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
        let mut state = ApplyPatchResponsesState::new(&codex(), request, request);
        assert!(state.active());
        let lines: &[&[u8]] = &[
            b"event: response.output_item.done",
            br#"data:   { "type":"response.output_item.done", "output_index":0, "item":{"type":"custom_tool_call","id":"a","name":"apply_patch","input":"raw"}}  "#,
            b"",
            b"event: response.completed",
            br#"data:  { "type":"response.completed", "sequence_number":8,"response":{"output":[{"type":"custom_tool_call","id":"a","name":"apply_patch","input":"raw"}]}} "#,
        ];
        for line in lines {
            let (out, error) = state.stream(line);
            assert!(error.is_none(), "{error:?}");
            if line.starts_with(b"event:") {
                continue;
            }
            assert!(!out.is_empty() && out.last().unwrap() == line, "{out:?}");
        }
    }

    #[test]
    fn dispatcher_keys_and_final_do_not_invent_a_preview() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"lookup"}]}]}"#;
        for key in [
            r#""output_index":0"#,
            r#""call_id":"c""#,
            r#""item_id":"a""#,
        ] {
            for terminal_only in [false, true] {
                let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
                state.add_dispatcher("n", "n");
                assert_ok(state.transform(br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"n","namespace":"n","arguments":""}}"#));
                let delta = format!(
                    r#"{{"type":"response.function_call_arguments.delta",{key},"delta":{}}}"#,
                    serde_json::to_string(r#"{"name":"apply_patch","arguments":{"input":"p"}}"#)
                        .unwrap()
                );
                assert_ok(state.transform(delta.as_bytes()));
                let item = r#"{"type":"function_call","id":"a","call_id":"c","name":"apply_patch","namespace":"n","arguments":"{\"input\":\"p\"}"}"#;
                let (out, error) = if terminal_only {
                    state.transform(
                        format!(
                            r#"{{"type":"response.completed","response":{{"output":[{item}]}}}}"#
                        )
                        .as_bytes(),
                    )
                } else {
                    state.transform(
                        format!(
                            r#"{{"type":"response.output_item.done","output_index":0,"item":{item}}}"#
                        )
                        .as_bytes(),
                    )
                };
                assert!(error.is_none(), "{key} {terminal_only} {error:?} {out:?}");
                assert!(
                    out.iter().any(|event| field(event, "type") == "response.custom_tool_call_input.done"),
                    "{key} {terminal_only} {out:?}"
                );
                assert!(
                    out.iter().all(
                        |event| field(event, "type") != "response.custom_tool_call_input.delta"
                    ),
                    "fabricated dispatcher preview: {out:?}"
                );
                assert!(state.bridge.finish().is_ok());
                if !terminal_only {
                    assert!(state.finish().is_err(), "{key}");
                }
            }
        }
    }

    #[test]
    fn chat_function_preference_keeps_ordinary_arguments() {
        let original = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","function":{"name":"apply_patch"}}]}"#;
        let declarations = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"apply_patch"}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai(), original, declarations);
        let raw = br#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","name":"apply_patch","arguments":"ordinary"}}"#;
        let out = assert_ok(state.transform(raw));
        assert_eq!(out, vec![raw.to_vec()]);
    }

    #[test]
    fn request_chat_preference_keeps_ordinary_parameters() {
        let original = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","function":{"name":"apply_patch","parameters":{"type":"object","properties":{"x":{"type":"integer"}}}}}]}"#;
        let body = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"apply_patch","parameters":{"type":"object","properties":{"x":{"type":"integer"}}}}]}"#;
        let out = super::normalize_apply_patch_responses_request(body, Some(original)).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert!(
            gjson::get(text, "tools.0.parameters.properties.x").exists(),
            "{text}"
        );
        assert!(
            !gjson::get(text, "tools.0.parameters.properties.input").exists(),
            "{text}"
        );
    }

    #[test]
    fn omitted_dispatcher_arguments_come_from_the_source() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        for late_name in ["n", "apply_patch"] {
            let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
            state.add_dispatcher("n", "n");
            for raw in [
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"n","arguments":""}}"#,
                r#"{"type":"response.function_call_arguments.delta","output_index":0,"item_id":"a","delta":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#,
            ] {
                assert_ok(state.transform(raw.as_bytes()));
            }
            let done = format!(
                r#"{{"type":"response.output_item.done","output_index":0,"item":{{"type":"function_call","id":"a","call_id":"c","name":{late_name},"namespace":"n"}}}}"#,
                late_name = serde_json::to_string(late_name).unwrap()
            );
            let out = assert_ok(state.transform(done.as_bytes()));
            assert_eq!(
                field(out.last().unwrap(), "item.input"),
                "p",
                "{late_name} {out:?}"
            );
        }
    }

    #[test]
    fn closed_response_ignores_later_events() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        state.add_dispatcher("n", "n");
        assert_ok(state.transform(br#"{"type":"response.completed","response":{"output":[{"type":"function_call","id":"a","call_id":"c","namespace":"n","name":"apply_patch","arguments":"{\"input\":\"p\"}"}]}}"#));
        let (out, error) = state.transform(br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","call_id":"changed","name":"n","arguments":""}}"#);
        assert!(out.is_empty() && error.is_none(), "{out:?} {error:?}");
        assert!(state.finish().is_ok());
    }

    #[test]
    fn transport_terminal_distinguishes_json_completion() {
        let request = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
        let complete = br#"data: {"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"apply_patch","arguments":"{\"input\":\"p\"}"}}"#;
        for json_terminal in [false, true] {
            let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
            assert_ok(state.stream(complete));
            if json_terminal {
                assert_ok(
                    state
                        .stream(br#"data: {"type":"response.completed","response":{"output":[]}}"#),
                );
            }
            let marker = b"data:   [DONE]  ";
            let (out, error) = state.stream(marker);
            if !json_terminal {
                assert!(
                    error.is_some()
                        && out.len() == 1
                        && joined(&out)
                            .windows(24)
                            .any(|window| window == br#""type":"response.failed""#),
                    "{out:?} {error:?}"
                );
            } else {
                assert!(
                    error.is_none() && out == vec![marker.to_vec()],
                    "{out:?} {error:?}"
                );
            }
            let lines: &[&[u8]] = &[
                complete,
                b"event: response.completed",
                b"",
                b": keepalive",
                marker,
            ];
            for line in lines {
                let (out, error) = state.stream(line);
                assert!(error.is_none() && out.is_empty(), "{out:?} {error:?}");
            }
            let (out, error) =
                state.transform(br#"{"type":"response.completed","response":{"output":[]}}"#);
            assert!(error.is_none() && out.is_empty(), "{out:?} {error:?}");
        }
    }

    #[test]
    fn failed_transport_does_not_repeat_the_failure() {
        let request = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        assert_ok(state.stream(br#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","name":"apply_patch","arguments":""}}"#));
        let (out, error) = state.stream(b"data: [DONE]");
        assert!(
            error.is_some()
                && out.len() == 1
                && String::from_utf8_lossy(&out[0]).contains(r#""type":"response.failed""#),
            "{out:?} {error:?}"
        );
        let lines: &[&[u8]] = &[
            b"data: [DONE]",
            br#"data: {"type":"response.completed","response":{"output":[]}}"#,
            b"event: response.completed",
        ];
        for line in lines {
            let (out, error) = state.stream(line);
            assert!(out.is_empty() && error.is_none(), "{out:?} {error:?}");
        }
    }

    #[test]
    fn inactive_transport_preserves_source_bytes() {
        let request = br#"{"tools":[{"type":"function","name":"apply_patch"}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        assert!(!state.active());
        let lines: &[&[u8]] = &[
            b"data: [DONE]",
            br#"data:  { "type":"response.completed", "response":{"output":[]}} "#,
            b"event: response.completed",
            b"",
            b"data: [DONE]",
        ];
        for line in lines {
            let (out, error) = state.stream(line);
            assert!(
                error.is_none() && out == vec![line.to_vec()],
                "{out:?} {error:?}"
            );
        }
    }

    #[test]
    fn retained_dispatcher_provenance_detects_conflicting_children() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"lookup"}]}]}"#;
        let patch = r#"{"name":"apply_patch","arguments":{"input":"p"}}"#;
        let ordinary = r#"{"name":"lookup","arguments":{"input":"p"}}"#;
        let cases = [
            ("patch_then_ordinary", "", vec![patch, ordinary], "n", true),
            ("ordinary_then_patch", "", vec![ordinary, patch], "n", true),
            (
                "conflicting_inputs",
                "",
                vec![patch, r#"{"name":"apply_patch","arguments":{"input":"q"}}"#],
                "n",
                true,
            ),
            (
                "full_source_conflicts_with_snapshot",
                patch,
                vec![ordinary],
                "n",
                true,
            ),
            (
                "full_source_conflicts_with_child",
                patch,
                vec![],
                "lookup",
                true,
            ),
            (
                "ordinary_child_is_not_patch",
                "",
                vec![ordinary],
                "n",
                false,
            ),
            (
                "ordinary_arguments_are_not_dispatcher_provenance",
                "",
                vec![
                    r#"{"name":"lookup","arguments":{"name":"apply_patch","arguments":{"input":"not patch"}}}"#,
                ],
                "n",
                false,
            ),
        ];
        for (label, delta, wrappers, final_name, should_fail) in cases {
            let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
            state.add_dispatcher("n", "n");
            assert_ok(state.transform(br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","name":"n","arguments":""}}"#));
            if !delta.is_empty() {
                let event = format!(
                    r#"{{"type":"response.function_call_arguments.delta","item_id":"a","delta":{}}}"#,
                    serde_json::to_string(delta).unwrap()
                );
                assert_ok(state.transform(event.as_bytes()));
            }
            for wrapper in &wrappers {
                let remembered = format!(
                    r#"{{"type":"response.function_call_arguments.done","item_id":"a","arguments":{}}}"#,
                    serde_json::to_string(wrapper).unwrap()
                );
                state.remember_dispatcher_arguments(remembered.as_bytes());
                let restored = gjson::parse(wrapper).get("arguments").json().to_owned();
                let transformed = format!(
                    r#"{{"type":"response.function_call_arguments.done","item_id":"a","arguments":{}}}"#,
                    serde_json::to_string(&restored).unwrap()
                );
                assert_ok(state.transform(transformed.as_bytes()));
            }
            let final_event = format!(
                r#"{{"type":"response.output_item.done","output_index":0,"item":{{"type":"function_call","id":"a","call_id":"late","name":{},"namespace":"n"}}}}"#,
                serde_json::to_string(final_name).unwrap()
            );
            let (out, error) = state.transform(final_event.as_bytes());
            if should_fail {
                assert!(
                    error.is_some()
                        && out.len() == 1
                        && field(&out[0], "type") == "response.failed",
                    "{label} {out:?} {error:?}"
                );
            } else {
                let expected = gjson::parse(wrappers.last().unwrap())
                    .get("arguments")
                    .json()
                    .to_owned();
                assert!(
                    error.is_none() && !out.is_empty(),
                    "{label} {out:?} {error:?}"
                );
                let last = out.last().unwrap();
                assert_eq!(field(last, "item.name"), "lookup", "{label}");
                assert_eq!(field(last, "item.namespace"), "n", "{label}");
                assert_eq!(field(last, "item.arguments"), expected, "{label}");
                assert!(
                    !String::from_utf8_lossy(&joined(&out)).contains("custom_tool_call"),
                    "{label}"
                );
            }
        }
    }

    #[test]
    fn dispatcher_snapshots_are_retained_on_every_matched_record() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        for discover in 0..3 {
            let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
            state.add_dispatcher("n", "n");
            for index in 0..3 {
                assert_ok(state.transform(format!(r#"{{"type":"response.output_item.added","output_index":{index},"item":{{"type":"function_call","id":"i{index}","call_id":"c{index}","name":"n","arguments":""}}}}"#).as_bytes()));
            }
            let original = br#"{"type":"response.function_call_arguments.done","output_index":0,"item_id":"i1","call_id":"c2","arguments":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#;
            state.remember_dispatcher_arguments(original);
            for key in ["item:i0", "item:i1", "item:i2"] {
                let index = state.by_dispatcher_key[key];
                assert_eq!(state.records[index].snapshots.len(), 1, "{key}");
                assert_eq!(state.records[index].snapshots[0], original, "{key}");
            }
            assert_ok(state.transform(br#"{"type":"response.function_call_arguments.done","output_index":0,"item_id":"i1","call_id":"c2","arguments":"{\"input\":\"p\"}"}"#));
            let done = format!(
                r#"{{"type":"response.output_item.done","output_index":{discover},"item":{{"type":"function_call","id":"i{discover}","call_id":"c{discover}","name":"n"}}}}"#
            );
            let (out, error) = state.transform(done.as_bytes());
            assert!(
                error.is_some() && out.len() == 1 && field(&out[0], "type") == "response.failed",
                "{discover} {out:?} {error:?}"
            );
        }
    }

    #[test]
    fn ordinary_progress_is_not_rewritten() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"lookup"}]}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        state.add_dispatcher("n", "n");
        for raw in [
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","name":"lookup","namespace":"n","arguments":""}}"#,
            r#"{"type":"response.function_call_arguments.delta","item_id":"a","delta":"{\"x\":1}"}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"a","arguments":"{\"x\":1}"}"#,
            r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"lookup","namespace":"n","arguments":"{\"x\":1}"}}"#,
        ] {
            state.remember_dispatcher_event(raw.as_bytes());
            let out = assert_ok(state.transform(raw.as_bytes()));
            assert_eq!(out, vec![raw.as_bytes().to_vec()], "{out:?}");
        }
    }

    #[test]
    fn dispatcher_lifecycle_retains_provenance_until_close() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        for close_at in ["response", "sentinel", "upstream_failure", "local_failure"] {
            let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
            state.add_dispatcher("n", "n");
            for raw in [
                r#"{"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","id":"a"}}"#,
                r#"{"type":"response.function_call_arguments.done","item_id":"a","arguments":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#,
            ] {
                let mut original = raw.as_bytes().to_vec();
                state.remember_dispatcher_event(&original);
                let (out, error) = state.transform(&original);
                let index = state.by_dispatcher_key["item:a"];
                assert!(
                    error.is_none() && out.is_empty() && state.records[index].namespace.is_empty(),
                    "{out:?} {error:?}"
                );
                original.fill(b'x');
            }
            let out = assert_ok(state.transform(br#"{"type":"response.output_item.done","output_index":2,"item":{"type":"function_call","id":"a","call_id":"c","name":"n"}}"#));
            assert_eq!(field(out.last().unwrap(), "item.input"), "p", "{out:?}");
            let call = state.by_dispatcher_key["item:a"];
            assert!(
                state.records[call].completed
                    && state.records[call].namespace == "n"
                    && state.records[call].index == 2,
                "{close_at}"
            );
            assert_eq!(state.by_dispatcher_key.get("call:c"), Some(&call));
            assert_eq!(state.by_dispatcher_key.get("index:2"), Some(&call));
            assert_eq!(state.records[call].snapshots.len(), 1);
            assert!(state.bridge.finish().is_ok());
            assert!(state.finish().is_err());
            assert_eq!(state.by_dispatcher_key.get("item:a"), Some(&call));
            match close_at {
                "response" => {
                    let out = assert_ok(state.transform(br#"{"type":"response.completed","response":{"output":[{"type":"function_call","id":"a","call_id":"c","name":"n"}]}}"#));
                    assert_eq!(out.len(), 1);
                    assert_eq!(field(&out[0], "response.output.0.input"), "p", "{out:?}");
                }
                "sentinel" => {
                    let (out, error) = state.stream(b"data: [DONE]");
                    assert!(
                        error.is_some()
                            && out.len() == 1
                            && String::from_utf8_lossy(&out[0])
                                .contains(r#""type":"response.failed""#),
                        "{out:?} {error:?}"
                    );
                }
                "upstream_failure" => {
                    assert_ok(
                        state.transform(br#"{"type":"response.failed","response":{"output":[]}}"#),
                    );
                }
                "local_failure" => {
                    let (out, error) = state.transform(br#"{"type":"response.output_item.done","output_index":3,"item":{"type":"function_call","id":"a","name":"n"}}"#);
                    assert!(error.is_some(), "{out:?}");
                }
                _ => unreachable!(),
            }
            assert!(
                state.by_dispatcher_key.is_empty()
                    && state.records.is_empty()
                    && state.upstream.is_none(),
                "{close_at}"
            );
            let (out, error) = state.transform(br#"{"type":"response.output_item.done","output_index":2,"item":{"type":"function_call","id":"a","name":"n"}}"#);
            assert!(error.is_none() && out.is_empty(), "{out:?} {error:?}");
        }
    }

    #[test]
    fn late_child_keeps_namespace_and_input() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        state.add_dispatcher("n", "n");
        let mut out = Vec::new();
        for raw in [
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","name":"n","arguments":""}}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"a","arguments":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#,
            r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"apply_patch"}}"#,
        ] {
            state.remember_dispatcher_event(raw.as_bytes());
            out.extend(assert_ok(state.transform(raw.as_bytes())));
        }
        assert!(!out.is_empty());
        assert_eq!(field(out.last().unwrap(), "item.namespace"), "n", "{out:?}");
        assert_eq!(field(out.last().unwrap(), "item.input"), "p", "{out:?}");
    }

    #[test]
    fn terminal_source_identity_uses_the_original_item() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        state.add_dispatcher("n", "n");
        for raw in [
            r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"a","name":"n","arguments":""}}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"a","arguments":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#,
            r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","id":"a","call_id":"c","name":"n"}}"#,
        ] {
            state.remember_dispatcher_event(raw.as_bytes());
            assert_ok(state.transform(raw.as_bytes()));
        }
        state.remember_dispatcher_event(br#"{"type":"response.completed","response":{"output":[{"type":"message","id":"removed"},{"type":"function_call","id":"a","call_id":"c","name":"n","namespace":"other"}]}}"#);
        let (out, error) = state.transform(br#"{"type":"response.completed","response":{"output":[{"type":"function_call","id":"a","call_id":"c","name":"n","namespace":"n"}]}}"#);
        assert!(
            error.is_some() && out.len() == 1 && field(&out[0], "type") == "response.failed",
            "{out:?} {error:?}"
        );
    }

    #[test]
    fn index_only_terminal_restores_the_child() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        state.add_dispatcher("n", "n");
        for raw in [
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call"}}"#,
            r#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#,
            r#"{"type":"response.function_call_arguments.done","output_index":0,"item_id":"a","call_id":"c","arguments":"{\"name\":\"apply_patch\",\"arguments\":{\"input\":\"p\"}}"}"#,
        ] {
            state.remember_dispatcher_event(raw.as_bytes());
            assert_ok(state.transform(raw.as_bytes()));
        }
        let terminal = br#"{"type":"response.completed","response":{"output":[{"type":"function_call","name":"n"}]}}"#;
        state.remember_dispatcher_event(terminal);
        let out = assert_ok(state.transform(terminal));
        assert_eq!(
            field(out.last().unwrap(), "response.output.0.input"),
            "p",
            "{out:?}"
        );
    }

    #[test]
    fn completed_ordinary_delta_stays_ordinary() {
        let request = br#"{"tools":[{"type":"namespace","name":"n","tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"lookup"}]}]}"#;
        let mut state = ApplyPatchResponsesState::new(&openai_response(), request, request);
        state.add_dispatcher("n", "n");
        for raw in [
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","name":"n"}}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"a","arguments":"{\"name\":\"lookup\",\"arguments\":{\"x\":1}}"}"#,
            r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"a","call_id":"c","name":"n"}}"#,
        ] {
            state.remember_dispatcher_event(raw.as_bytes());
            assert_ok(state.transform(raw.as_bytes()));
        }
        let raw = br#"{"type":"response.function_call_arguments.delta","item_id":"a","delta":"ordinary"}"#;
        state.remember_dispatcher_event(raw);
        let out = assert_ok(state.transform(raw));
        assert_eq!(out, vec![raw.to_vec()]);
    }

    #[test]
    fn source_terminal_is_required_on_eof() {
        let request = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#;
        for mode in ["empty", "arguments", "item"] {
            let mut state = ApplyPatchResponsesState::new(&codex(), request, request);
            if mode != "empty" {
                assert_ok(state.transform(br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"a","name":"apply_patch","arguments":""}}"#));
                let event = if mode == "item" {
                    r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"a","name":"apply_patch","arguments":"{\"input\":\"p\"}"}}"#
                } else {
                    r#"{"type":"response.function_call_arguments.done","item_id":"a","arguments":"{\"input\":\"p\"}"}"#
                };
                assert_ok(state.transform(event.as_bytes()));
            }
            let (out, error) = state.finish_stream();
            assert!(
                error.is_some()
                    && out.len() == 1
                    && String::from_utf8_lossy(&out[0]).contains(r#""type":"response.failed""#),
                "{mode} {out:?} {error:?}"
            );
            let (out, error) = state.finish_stream();
            assert!(
                out.is_empty() && error.is_none(),
                "{mode} {out:?} {error:?}"
            );
        }
    }

    #[test]
    fn inactive_eof_and_done_preserve_bytes() {
        for request in [
            "{}",
            r#"{"tools":[{"type":"function","name":"apply_patch"}]}"#,
        ] {
            let mut state =
                ApplyPatchResponsesState::new(&codex(), request.as_bytes(), request.as_bytes());
            let (out, error) = state.finish_stream();
            assert!(out.is_empty() && error.is_none(), "{out:?} {error:?}");
            let line = b"data: [DONE]";
            let (out, error) = state.stream(line);
            assert!(
                error.is_none() && out == vec![line.to_vec()],
                "{out:?} {error:?}"
            );
        }
    }
}
