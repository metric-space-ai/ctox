// ref: internal/client/codex/tool-schema/tool_schema_integer_fields_test.go @ d7914afd
// License: MIT (upstream); modifications AGPL-3.0-only

use super::normalize_codex_tool_integer_types;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn normalize(body: &str, user_agent: &str) -> Vec<u8> {
    let headers = BTreeMap::from([("uSeR-aGeNt".into(), vec![user_agent.into()])]);
    normalize_codex_tool_integer_types(body.as_bytes(), &headers)
}

fn insert_path(node: &mut Value, parts: &[&str], replacement: Value) {
    if parts.is_empty() {
        *node = replacement;
        return;
    }
    if let Ok(index) = parts[0].parse::<usize>() {
        if !node.is_array() {
            *node = json!([]);
        }
        let items = node.as_array_mut().unwrap();
        items.resize_with(items.len().max(index + 1), || Value::Null);
        insert_path(&mut items[index], &parts[1..], replacement);
    } else {
        if !node.is_object() {
            *node = json!({});
        }
        let child = node
            .as_object_mut()
            .unwrap()
            .entry(parts[0])
            .or_insert(Value::Null);
        insert_path(child, &parts[1..], replacement);
    }
}

fn schema(fields: &[&str], field_type: &str) -> Value {
    let mut schema = json!({
        "type":"object",
        "properties":{"unrelated":{"type":"number","default":1.5}}
    });
    for field in fields {
        let parts: Vec<_> = field.split('.').collect();
        let property = json!({
            "type":serde_json::from_str::<Value>(field_type).unwrap(),
            "description":"Keep metadata", "minimum":0, "default":1
        });
        insert_path(&mut schema["properties"], &parts, property);
    }
    schema
}

fn request(name: &str, fields: &[&str], field_type: &str) -> String {
    serde_json::to_string(&json!({
        "tools":[{
            "type":"function", "name":name, "parameters":schema(fields, field_type),
            "output_schema":{"properties":{"wall_time_seconds":{"type":"number"}}}
        }],
        "input":[{"type":"function_call","arguments":"{\"limit\":1.5}"}]
    }))
    .unwrap()
}

#[test]
fn candidate_integer_source_fields_follow_upstream_exact_paths_and_client_gate() {
    let cases: &[(&str, &[&str])] = &[
        (
            "test_sync_tool",
            &[
                "barrier.properties.participants",
                "barrier.properties.timeout_ms",
            ],
        ),
        ("create_goal", &["token_budget"]),
        ("get_channels", &["limit"]),
        ("list_threads", &["limit", "max_chars_per_post"]),
        ("search_posts", &["limit", "max_chars_per_post"]),
        ("read_thread", &["limit", "max_chars_per_post"]),
        ("read_post", &["offset_chars", "limit_chars"]),
        ("memories__list", &["max_results"]),
        ("memories__read", &["line_offset", "max_lines"]),
        ("memories__search", &["context_lines", "max_results"]),
        ("history__list_windows", &["limit"]),
        ("history__list_items", &["limit", "max_chars_per_item"]),
        ("history__read_item", &["offset_chars", "limit_chars"]),
        ("history__search_contents", &["limit"]),
        ("notes__list_files_by_prefix", &["max_results"]),
        (
            "notes__read_file",
            &[
                "start_line",
                "stop_line",
                "start_line.anyOf.0",
                "stop_line.anyOf.0",
            ],
        ),
        (
            "notes__search_contents",
            &["max_matches_per_file", "max_files"],
        ),
        ("image_gen__imagegen", &["num_last_images_to_include"]),
        (
            "web__run",
            &[
                "search_query.items.properties.recency",
                "image_query.items.properties.recency",
                "open.items.properties.lineno",
                "click.items.properties.id",
                "screenshot.items.properties.pageno",
                "weather.items.properties.duration",
                "sports.items.properties.num_games",
            ],
        ),
        ("collaboration__wait_agent", &["timeout_ms"]),
        ("multi_agent_v1__wait_agent", &["timeout_ms"]),
        ("functions__create_goal", &["token_budget"]),
        ("collab__read_post", &["offset_chars", "limit_chars"]),
        ("collaboration__get_channels", &["limit"]),
        (
            "collaboration__list_threads",
            &["limit", "max_chars_per_post"],
        ),
        (
            "collaboration__search_posts",
            &["limit", "max_chars_per_post"],
        ),
        (
            "collaboration__read_thread",
            &["limit", "max_chars_per_post"],
        ),
        ("collaboration__read_post", &["offset_chars", "limit_chars"]),
    ];
    for &(name, fields) in cases {
        for field_type in [
            r#""number""#,
            r#"["number","integer","null"]"#,
            r#""integer""#,
            r#""string""#,
        ] {
            let input = request(name, fields, field_type);
            for ua in ["codex-tui/0.154.0", "curl/8.7.1", ""] {
                let expected_type = match (ua, field_type) {
                    ("codex-tui/0.154.0", r#""number""#) => r#""integer""#,
                    ("codex-tui/0.154.0", r#"["number","integer","null"]"#) => {
                        r#"["integer","null"]"#
                    }
                    _ => field_type,
                };
                let want = request(name, fields, expected_type);
                let actual = normalize(&input, ua);
                assert_eq!(actual, want.as_bytes(), "{name}, {ua}, {field_type}");
                assert_eq!(normalize(std::str::from_utf8(&actual).unwrap(), ua), actual);
            }
        }
    }
}

#[test]
fn candidate_integer_namespace_and_protocol_forms_preserve_unrelated_fields() {
    let schema = r#"{"type":"object","properties":{"line_offset":{"type":"number"},"max_lines":{"type":["number","null"]},"max_tokens":{"type":"number"}}}"#;
    let cases = [
        format!(
            r#"{{"tools":[{{"type":"function","name":"memories__read","parameters":{schema}}}]}}"#
        ),
        format!(
            r#"{{"tools":[{{"type":"function","function":{{"name":"memories__read","parameters":{schema}}}}}]}}"#
        ),
        format!(r#"{{"tools":[{{"name":"memories__read","input_schema":{schema}}}]}}"#),
        format!(
            r#"{{"tools":[{{"function_declarations":[{{"name":"memories__read","parameters":{schema}}}]}}]}}"#
        ),
        format!(
            r#"{{"tools":[{{"functionDeclarations":[{{"name":"memories__read","parametersJsonSchema":{schema}}}]}}]}}"#
        ),
        format!(
            r#"{{"tools":[{{"type":"namespace","name":"memories","tools":[{{"type":"function","name":"read","parameters":{schema}}}]}}]}}"#
        ),
        format!(
            r#"{{"input":[{{"type":"additional_tools","tools":[{{"type":"namespace","name":"memories","tools":[{{"type":"function","name":"read","parameters":{schema}}}]}}]}}]}}"#
        ),
    ];
    for input in cases {
        let want = input
            .replace(
                r#""line_offset":{"type":"number"}"#,
                r#""line_offset":{"type":"integer"}"#,
            )
            .replace(
                r#""max_lines":{"type":["number","null"]}"#,
                r#""max_lines":{"type":["integer","null"]}"#,
            );
        assert_eq!(normalize(&input, "Codex/1.0"), want.as_bytes());
    }
}

#[test]
fn candidate_integer_unknown_namespaces_and_unproven_paths_are_byte_transparent() {
    for (name, namespace) in [
        ("unknown_tool", ""),
        ("mcp__server__read_post", ""),
        ("read", ""),
        ("run", ""),
        ("imagegen", ""),
        ("read_post", "user_tools"),
        ("wait_agent", "mcp__server"),
        ("read", "skills"),
        ("read", "user_tools"),
        ("read_post", "multi_agent_v1"),
        ("read_post", "arbitrary_collaboration"),
    ] {
        let tool = format!(
            r#"{{"type":"function","name":"{name}","parameters":{{"properties":{{"limit":{{"type":"number"}},"offset_chars":{{"type":"number"}},"line_offset":{{"type":"number"}},"timeout_ms":{{"type":"number"}}}}}}}}"#
        );
        let input = if namespace.is_empty() {
            format!(r#"{{"tools":[{tool}]}}"#)
        } else {
            format!(r#"{{"tools":[{{"type":"namespace","name":"{namespace}","tools":[{tool}]}}]}}"#)
        };
        assert_eq!(
            normalize(&input, "codex"),
            input.as_bytes(),
            "{namespace}/{name}"
        );
    }
    let input = r#"{"tools":[{"name":"test_sync_tool","parameters":{"properties":{"barrier.participants":{"type":"number"},"other":{"properties":{"participants":{"type":"number"}}},"barrier":{"properties":{"participants":{"type":"number"},"ratio":{"type":"number"}}}}}}]}"#;
    let want = input.replace(
        r#""barrier":{"properties":{"participants":{"type":"number"}"#,
        r#""barrier":{"properties":{"participants":{"type":"integer"}"#,
    );
    assert_eq!(normalize(input, "codex"), want.as_bytes());
}

#[test]
fn candidate_integer_history_notes_source_fixture_round_trips_in_three_forms() {
    let fixture =
        include_str!("../../../client/codex/tool-schema/testdata/history_notes_tools.json");
    let root = gjson::parse(fixture);
    let namespaces = root.get("tools");
    assert_eq!(namespaces.array().len(), 7);
    for namespace in namespaces.array() {
        let tool = namespace.get("tools.0");
        let name = format!(
            "{}__{}",
            namespace.get("name").str(),
            tool.get("name").str()
        );
        let name_json = serde_json::to_string(&name).unwrap();
        for body in [
            format!(r#"{{"tools":[{}]}}"#, namespace.json()),
            format!(
                r#"{{"tools":[{{"name":{name_json},"parameters":{}}}]}}"#,
                tool.get("parameters").json()
            ),
            format!(
                r#"{{"input":[{{"type":"additional_tools","tools":[{}]}}]}}"#,
                namespace.json()
            ),
        ] {
            let input = body.replace(r#""type": "integer""#, r#""type": "number""#);
            for ua in ["codex", "curl", ""] {
                let want = if ua == "codex" { &body } else { &input };
                let actual = normalize(&input, ua);
                assert_eq!(actual, want.as_bytes(), "{name}, {ua}");
                assert_eq!(normalize(std::str::from_utf8(&actual).unwrap(), ua), actual);
            }
        }
        for unknown in [
            tool.get("name").str().to_owned(),
            format!("mcp__server__{name}"),
            format!("user__{name}"),
        ] {
            let name_json = serde_json::to_string(&unknown).unwrap();
            let parameters = tool
                .get("parameters")
                .json()
                .replace(r#""type": "integer""#, r#""type": "number""#);
            let input =
                format!(r#"{{"tools":[{{"name":{name_json},"parameters":{parameters}}}]}}"#);
            assert_eq!(normalize(&input, "codex"), input.as_bytes(), "{unknown}");
        }
    }
}

#[test]
fn candidate_integer_notes_explicit_union_paths_preserve_signed_values_and_other_branches() {
    let input = r#"{"tools":[{"name":"notes__read_file","parameters":{"type":"object","properties":{"start_line":{"anyOf":[{"type":"number"},{"type":"null"}],"default":-3},"stop_line":{"anyOf":[{"type":"number"},{"type":"number"}],"default":-1},"ratio":{"anyOf":[{"type":"number"},{"type":"null"}]},"other":{"properties":{"start_line":{"anyOf":[{"type":"number"}]}}}},"required":["path"]}}],"input":[{"type":"function_call","arguments":"{\"start_line\":-3,\"stop_line\":-1}"}]}"#;
    let want = input
        .replace(
            r#""start_line":{"anyOf":[{"type":"number"},{"type":"null"}],"default":-3}"#,
            r#""start_line":{"anyOf":[{"type":"integer"},{"type":"null"}],"default":-3}"#,
        )
        .replace(
            r#""stop_line":{"anyOf":[{"type":"number"},{"type":"number"}],"default":-1}"#,
            r#""stop_line":{"anyOf":[{"type":"integer"},{"type":"number"}],"default":-1}"#,
        );
    assert_eq!(normalize(input, "codex"), want.as_bytes());
}

#[test]
fn candidate_integer_only_changes_the_first_duplicate_target_and_preserves_raw_siblings() {
    let input = r#"{"metadata":{"minimum":900719925474099312345,"escaped":"\u5de5","float":1.00e+9},"tools":[{"name":"history__list_windows","parameters":{"properties":{"unrelated":{"type":"number","default":1.00},"limit":{"type":"number","default":1.00},"limit":{"type":"number","default":9}}},"output_schema":{"type":"number"}}],"tools":[{"name":"history__list_windows","parameters":{"properties":{"limit":{"type":"number"}}}}]}"#;
    let want = input.replacen(
        r#""limit":{"type":"number","default":1.00}"#,
        r#""limit":{"type":"integer","default":1.00}"#,
        1,
    );
    assert_eq!(normalize(input, "codex"), want.as_bytes());
}
