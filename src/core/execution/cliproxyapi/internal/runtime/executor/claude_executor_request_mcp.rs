// ref: internal/runtime/executor/claude_executor_request.go:2296-2470 @ 02fc76b149a917d07db6c1f588f47341bb29c7be
// Port-Status: candidate; request-local MCP passthrough recovery
// License: MIT (upstream); modifications AGPL-3.0-only
use super::super::helps::is_claude_mcp_tool_name;
use super::{is_claude_server_tool_type, mutate_response_tool_names, mutate_stream_tool_name};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Default)]
pub(crate) struct ClaudeMcpAliasResolver {
    reverse: HashMap<String, String>,
    virtual_servers: BTreeSet<String>,
    passthroughs: BTreeSet<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClaudeMcpAliasError;
impl ClaudeMcpAliasResolver {
    pub(crate) fn new(body: &[u8], reverse: &HashMap<String, String>) -> Self {
        let virtual_servers = reverse
            .keys()
            .filter(|name| is_claude_mcp_tool_name(name))
            .filter_map(|name| {
                name.strip_prefix("mcp__")?
                    .split_once("__")
                    .map(|(server, _)| server.to_owned())
            })
            .collect();
        let mut passthroughs = BTreeSet::new();
        if let Ok(body) = serde_json::from_slice::<Value>(body) {
            if let Some(tools) = body.get("tools").and_then(Value::as_array) {
                for tool in tools {
                    if tool
                        .get("type")
                        .and_then(Value::as_str)
                        .is_some_and(is_claude_server_tool_type)
                    {
                        continue;
                    }
                    if let Some(name) = tool.get("name").and_then(Value::as_str) {
                        if is_claude_mcp_tool_name(name) && !reverse.contains_key(name) {
                            passthroughs.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        Self {
            reverse: reverse.clone(),
            virtual_servers,
            passthroughs,
        }
    }
    fn resolve(&self, name: &str) -> Result<Option<String>, ClaudeMcpAliasError> {
        if let Some(original) = self.reverse.get(name) {
            return Ok(Some(original.clone()));
        }
        let Some((server, suffix)) = name
            .strip_prefix("mcp__")
            .and_then(|rest| rest.split_once("__"))
        else {
            return Ok(None);
        };
        if !self.virtual_servers.contains(server) || suffix.is_empty() {
            return Ok(None);
        }
        // Upstream recovery 0: retain the full declared caller-owned MCP name.
        if self.passthroughs.contains(suffix) {
            return Ok(Some(suffix.to_owned()));
        }
        // Existing upstream recoveries: full name without its prefix, then a
        // unique tool tail. Ambiguous tails never select an arbitrary namespace.
        if let Some(original) = self
            .passthroughs
            .iter()
            .find(|original| original.strip_prefix("mcp__") == Some(suffix))
        {
            return Ok(Some(original.clone()));
        }
        let mut matching = self.passthroughs.iter().filter(|original| {
            original
                .strip_prefix("mcp__")
                .and_then(|rest| rest.split_once("__"))
                .is_some_and(|(_, tail)| tail == suffix)
        });
        let first = matching.next();
        if matching.next().is_some() {
            return Err(ClaudeMcpAliasError);
        }
        Ok(first.cloned())
    }
    pub(crate) fn restore_response(&self, body: &[u8]) -> Result<Vec<u8>, ClaudeMcpAliasError> {
        let mut failure = None;
        let restored = mutate_response_tool_names(body, |name| match self.resolve(name) {
            Ok(name) => name,
            Err(error) => {
                failure = Some(error);
                None
            }
        });
        failure.map_or(Ok(restored), Err)
    }
    pub(crate) fn restore_line(&self, line: &[u8]) -> Result<Vec<u8>, ClaudeMcpAliasError> {
        let mut failure = None;
        let restored = mutate_stream_tool_name(line, |name| match self.resolve(name) {
            Ok(name) => name,
            Err(error) => {
                failure = Some(error);
                None
            }
        });
        failure.map_or(Ok(restored), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resolver(names: &[&str]) -> (ClaudeMcpAliasResolver, String) {
        let mut tools = vec![serde_json::json!({"name":"Bash"})];
        tools.extend(names.iter().map(|name| serde_json::json!({"name":name})));
        let body = serde_json::to_vec(&serde_json::json!({"tools":tools})).unwrap();
        let (mapped, reverse) =
            super::super::remap_claude_oauth_tool_names_with_secret(&body, "fixture-oauth");
        let server = reverse
            .keys()
            .next()
            .unwrap()
            .strip_prefix("mcp__")
            .unwrap()
            .split_once("__")
            .unwrap()
            .0
            .to_owned();
        (ClaudeMcpAliasResolver::new(&mapped, &reverse), server)
    }
    #[test]
    fn ambiguous_wrapped_tail_is_a_redacted_error_in_json_and_sse() {
        let (r, server) = resolver(&["mcp__first__query", "mcp__second__query"]);
        let name = format!("mcp__{server}__query");
        let body =
            serde_json::to_vec(&serde_json::json!({"content":[{"type":"tool_use","name":name}]}))
                .unwrap();
        assert_eq!(r.restore_response(&body), Err(ClaudeMcpAliasError));
        let line = format!(
            "data: {}",
            serde_json::json!({"type":"content_block_start","content_block":{"type":"tool_use","name":name}})
        );
        assert_eq!(r.restore_line(line.as_bytes()), Err(ClaudeMcpAliasError));
        assert_eq!(format!("{:?}", ClaudeMcpAliasError), "ClaudeMcpAliasError");
    }
    #[test]
    fn unknown_namespace_undeclared_name_and_malformed_input_stay_byte_identical() {
        let (r, server) = resolver(&["mcp__docs__query"]);
        for name in [
            "mcp__foreign__query".to_owned(),
            format!("mcp__{server}__undeclared"),
            "mcp__docs__query".to_owned(),
        ] {
            let body = format!(
                " {{\"content\":[{{\"type\":\"tool_use\",\"name\":{}}}]}} ",
                serde_json::to_string(&name).unwrap()
            )
            .into_bytes();
            assert_eq!(r.restore_response(&body).unwrap(), body);
        }
        assert_eq!(
            r.restore_line(b"data: malformed").unwrap(),
            b"data: malformed"
        );
    }
    #[test]
    fn current_declarations_do_not_leak_from_another_request() {
        let (a, server) = resolver(&["mcp__first__query"]);
        let (b, _) = resolver(&["mcp__second__query"]);
        let body=serde_json::to_vec(&serde_json::json!({"content":[{"type":"tool_use","name":format!("mcp__{server}__mcp__first__query")}]})).unwrap();
        let got: Value = serde_json::from_slice(&a.restore_response(&body).unwrap()).unwrap();
        assert_eq!(got["content"][0]["name"], "mcp__first__query");
        assert_eq!(b.restore_response(&body).unwrap(), body);
    }
    #[test]
    fn long_wrapped_name_restores_the_declared_name_and_nested_references() {
        let original = format!("mcp__docs__{}", "x".repeat(48));
        assert!(is_claude_mcp_tool_name(&original));
        let (r, server) = resolver(&[&original]);
        let name = format!("mcp__{server}__{original}");
        assert!(name.len() > 64);
        let body=serde_json::to_vec(&serde_json::json!({"content":[{"type":"tool_reference","tool_name":name},{"type":"tool_result","content":[{"type":"tool_reference","tool_name":name}]}]})).unwrap();
        let got: Value = serde_json::from_slice(&r.restore_response(&body).unwrap()).unwrap();
        assert_eq!(got["content"][0]["tool_name"], original);
        assert_eq!(got["content"][1]["content"][0]["tool_name"], original);
    }
    #[test]
    fn native_server_tools_are_not_caller_owned_passthroughs() {
        let body=br#"{"tools":[{"name":"Bash"},{"type":"web_search_20250305","name":"mcp__docs__query"}]}"#;
        let (mapped, reverse) =
            super::super::remap_claude_oauth_tool_names_with_secret(body, "fixture-oauth");
        let server = reverse
            .keys()
            .next()
            .unwrap()
            .strip_prefix("mcp__")
            .unwrap()
            .split_once("__")
            .unwrap()
            .0;
        let r = ClaudeMcpAliasResolver::new(&mapped, &reverse);
        assert_eq!(r.resolve(&format!("mcp__{server}__query")), Ok(None));
        let alias = reverse.keys().next().unwrap();
        assert_eq!(r.resolve(alias), Ok(Some("Bash".to_owned())));
    }
}
