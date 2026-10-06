// ref: internal/runtime/executor/helps/codex_tool_schema_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;

use super::{
    normalize_codex_tool_integer_types, normalize_codex_tool_integer_types_for_executor,
    normalize_codex_tool_schemas,
};
use serde_json::Value;

fn normalize(input: &str) -> Value {
    serde_json::from_slice(&normalize_codex_tool_schemas(input.as_bytes())).unwrap()
}

fn const_union(count: usize) -> String {
    let branches = (1..=count)
        .map(|index| format!(r#"{{"const":"m{index}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"type":"function","name":"choose","parameters":{{"type":"object","properties":{{"mode":{{"type":"string","oneOf":[{branches}]}}}}}}}}"#
    )
}

#[test]
fn matching_enum_drops_only_the_large_const_union() {
    let input = r#"{
        "model":"gpt-5.5",
        "tools":[{
            "type":"function",
            "name":"t1",
            "strict":true,
            "parameters":{
                "type":"object",
                "properties":{
                    "action":{
                        "type":"string",
                        "enum":["a","b","c","d","e","f","g","h"],
                        "oneOf":[
                            {"const":"a"},{"const":"b"},{"const":"c"},{"const":"d"},
                            {"const":"e"},{"const":"f"},{"const":"g"},{"const":"h"}
                        ],
                        "description":"Action"
                    },
                    "target":{"type":"string"}
                },
                "required":["action"]
            }
        }]
    }"#;
    let output = normalize(input);
    let action = &output["tools"][0]["parameters"]["properties"]["action"];
    assert!(action.get("oneOf").is_none());
    assert_eq!(action["enum"].as_array().unwrap().len(), 8);
    assert_eq!(action["type"], "string");
    assert_eq!(
        output["tools"][0]["parameters"]["properties"]["target"]["type"],
        "string"
    );
    assert_eq!(output["tools"][0]["parameters"]["required"][0], "action");
    assert_eq!(output["tools"][0]["strict"], true);
}

#[test]
fn dotted_and_colon_property_names_stay_literal() {
    for name in ["my.action", ":action"] {
        let property: Value = serde_json::from_str(&const_union_property()).unwrap();
        let mut properties = serde_json::Map::new();
        properties.insert(name.to_owned(), property);
        let input = serde_json::json!({
            "tools": [{
                "type": "function",
                "name": "t",
                "parameters": {"type": "object", "properties": properties}
            }]
        });
        let output = normalize(&serde_json::to_string(&input).unwrap());
        let properties = output["tools"][0]["parameters"]["properties"]
            .as_object()
            .unwrap();
        assert!(properties.get(name).unwrap().get("oneOf").is_none());
        assert_eq!(properties.len(), 1);
    }
}

fn const_union_property() -> String {
    r#"{"type":"string","enum":["1","2","3","4","5","6","7","8"],"oneOf":[{"const":"1"},{"const":"2"},{"const":"3"},{"const":"4"},{"const":"5"},{"const":"6"},{"const":"7"},{"const":"8"}]}"#.to_owned()
}

#[test]
fn equal_numeric_spellings_and_small_or_mixed_unions_stay() {
    let duplicate = r#"{"tools":[{"type":"function","name":"n","parameters":{"type":"object","properties":{"val":{"type":"number","oneOf":[{"const":1},{"const":1.0},{"const":2},{"const":3},{"const":4},{"const":5},{"const":6},{"const":7}]}}}}]}"#;
    assert_eq!(
        normalize_codex_tool_schemas(duplicate.as_bytes()),
        duplicate.as_bytes()
    );
    let small = format!(r#"{{"tools":[{}]}}"#, const_union(7));
    assert_eq!(
        normalize_codex_tool_schemas(small.as_bytes()),
        small.as_bytes()
    );
    let mixed = r#"{"tools":[{"type":"function","name":"m","parameters":{"type":"object","properties":{"mode":{"oneOf":[{"const":"a"},{"const":"b"},{"const":"c"},{"const":"d"},{"const":"e"},{"const":"f"},{"const":"g"},{"type":"string"}],"anyOf":[{"const":"a"}]}}}}]}"#;
    assert_eq!(
        normalize_codex_tool_schemas(mixed.as_bytes()),
        mixed.as_bytes()
    );
    let non_const = r#"{"tools":[{"type":"function","name":"m","parameters":{"type":"object","properties":{"mode":{"oneOf":[{"const":"a"},{"const":"b"},{"const":"c"},{"const":"d"},{"const":"e"},{"const":"f"},{"const":"g"},{"type":"string","const":"h"}]}}}}]}"#;
    assert_eq!(
        normalize_codex_tool_schemas(non_const.as_bytes()),
        non_const.as_bytes()
    );
}

#[test]
fn migrated_enum_preserves_large_integer_digits() {
    let large = "9007199254740993";
    let input = format!(
        r#"{{"tools":[{{"type":"custom","name":"id","parameters":{{"type":"object","properties":{{"id":{{"type":"integer","oneOf":[{{"const":{large}}},{{"const":1}},{{"const":2}},{{"const":3}},{{"const":4}},{{"const":5}},{{"const":6}},{{"const":7}}]}}}}}}}}]}}"#
    );
    let output = normalize_codex_tool_schemas(input.as_bytes());
    let parsed: Value = serde_json::from_slice(&output).unwrap();
    let id = &parsed["tools"][0]["parameters"]["properties"]["id"];
    assert!(id.get("oneOf").is_none());
    assert!(output
        .windows(large.len())
        .any(|window| window == large.as_bytes()));
    assert_eq!(id["enum"].as_array().unwrap().len(), 8);
    assert_eq!(id["type"], "integer");
}

#[test]
fn namespace_edit_preserves_unchanged_tool_bytes_and_outside_fields() {
    let simple = r#"{"type":"function","name":"read","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}"#;
    let union = const_union(8);
    let input = format!(
        "{{\r\n  \"input\": \"tool_search \\u5de5\", \"tools\": [\n\t{{\"type\":\"namespace\",\"name\":\"workspace\",\"tools\":[{simple}, {union}]}}, {simple}\n  ], \"metadata\": {{\"number\": 900719925474099312345, \"value\": 1.00e+9}}\r\n}}"
    );
    let output = normalize_codex_tool_schemas(input.as_bytes());
    let output_text = std::str::from_utf8(&output).unwrap();
    assert!(output_text.contains(simple));
    assert!(output_text.contains(r"\u5de5"));
    assert!(output_text.contains("900719925474099312345"));
    assert!(output_text.contains("1.00e+9"));
    assert!(!output_text.contains("\"oneOf\""));
    let again = normalize_codex_tool_schemas(&output);
    assert_eq!(again, output);
}

#[test]
fn unsupported_patterns_are_removed_only_from_schema_locations() {
    let input = r#"{
        "tools":[{
            "type":"function",
            "name":"artifact",
            "parameters":{
                "type":"object",
                "properties":{
                    "file_paths":{"type":"array","items":{"type":"string","pattern":"^[^\\0]*$","minLength":1}},
                    "asset_id":{"type":"string","pattern":"^[0-9a-f]{32}$"},
                    "hex_nul":{"type":"string","pattern":"^[^\\x00]*$"},
                    "unicode_escape":{"type":"string","pattern":"\u005cp{L}+"},
                    "real_schema":{"type":"string","pattern":"\\p{L}+"},
                    "regex_config":{"type":"object","default":{"pattern":"\\p{L}+"},"enum":[{"pattern":"\\p{N}+"}]}
                },
                "patternProperties":{
                    "^\\p{L}+$":{"type":"string"},
                    "^[a-z]+$":{"type":"number"}
                },
                "$defs":{"custom_type":{"type":"string","pattern":"\\p{L}+"}},
                "additionalProperties":{"type":"string","pattern":"\\p{N}+"}
            }
        }]
    }"#;
    let output = normalize(input);
    let parameters = &output["tools"][0]["parameters"];
    assert!(parameters["properties"]["file_paths"]["items"]
        .get("pattern")
        .is_none());
    assert_eq!(
        parameters["properties"]["file_paths"]["items"]["minLength"],
        1
    );
    assert_eq!(
        parameters["properties"]["asset_id"]["pattern"],
        "^[0-9a-f]{32}$"
    );
    assert_eq!(
        parameters["properties"]["hex_nul"]["pattern"],
        r"^[^\x00]*$"
    );
    assert!(parameters["properties"]["unicode_escape"]
        .get("pattern")
        .is_none());
    assert!(parameters["properties"]["real_schema"]
        .get("pattern")
        .is_none());
    assert_eq!(
        parameters["properties"]["regex_config"]["default"]["pattern"],
        r"\p{L}+"
    );
    assert_eq!(
        parameters["properties"]["regex_config"]["enum"][0]["pattern"],
        r"\p{N}+"
    );
    assert!(parameters["patternProperties"].get(r"^\p{L}+$").is_none());
    assert_eq!(
        parameters["patternProperties"]["^[a-z]+$"]["type"],
        "number"
    );
    assert!(parameters["$defs"]["custom_type"].get("pattern").is_none());
    assert!(parameters["additionalProperties"].get("pattern").is_none());
    assert_eq!(parameters["properties"]["asset_id"]["type"], "string");
}

#[test]
fn malformed_or_unchanged_payloads_keep_their_bytes() {
    let unchanged = [
        "",
        "{}",
        r#"{"tools":null}"#,
        r#"{"tools":[]}"#,
        r#"{"tools":"unchanged"}"#,
        r#"{"model":"gpt-5.6","tools":[{"type":"function","name":"t","parameters":{"type":"object","properties":{"n":{"type":"number"}}}}]}"#,
        r#"{"tools":[{"type":"function","name":"t","parameters":null}]}"#,
        r#"{"tools":["#,
    ];
    for input in unchanged {
        assert_eq!(
            normalize_codex_tool_schemas(input.as_bytes()),
            input.as_bytes(),
            "{input}"
        );
    }
    assert_eq!(normalize_codex_tool_schemas(&[0xff, 0xfe]), [0xff, 0xfe]);
}

fn codex_headers(user_agent: &str) -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([("uSeR-aGeNt".to_owned(), vec![format!("  {user_agent}  ")])])
}

fn integer_normalize(input: &str, user_agent: &str) -> Value {
    serde_json::from_slice(&normalize_codex_tool_integer_types(
        input.as_bytes(),
        &codex_headers(user_agent),
    ))
    .unwrap()
}

#[test]
fn integer_types_follow_codex_client_and_skip_codex_targets() {
    let input = r#"{"keep":1.00,"tools":[
        {"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"cmd":{"type":"string"},"yield_time_ms":{"type":"number"},"max_output_tokens":{"type":"number"},"timeout_ms":{"type":"number"}}}},
        {"type":"function","name":"write_stdin","parameters":{"type":"object","properties":{"session_id":{"type":"number"},"yield_time_ms":{"type":"number"},"max_output_tokens":{"type":"number"}}}},
        {"type":"function","name":"sleep","parameters":{"type":"object","properties":{"duration_ms":{"type":"number"}}}},
        {"type":"function","name":"wait_agent","parameters":{"type":"object","properties":{"timeout_ms":{"type":"number"}}}},
        {"type":"function","name":"wait","parameters":{"type":"object","properties":{"yield_time_ms":{"type":"number"},"max_tokens":{"type":"number"}}}},
        {"type":"function","name":"tool_search","parameters":{"type":"object","properties":{"limit":{"type":"number"}}}},
        {"type":"function","name":"test_sync_tool","parameters":{"type":"object","properties":{"sleep_before_ms":{"type":"number"},"sleep_after_ms":{"type":"number"},"participants":{"type":"number"},"timeout_ms":{"type":"number"}}}},
        {"type":"function","name":"unrelated_tool","parameters":{"type":"object","properties":{"timeout_ms":{"type":"number"}}}},
        {"type":"function","name":"mcp__server__sleep","parameters":{"type":"object","properties":{"duration_ms":{"type":"number"}}}},
        {"type":"function","function":{"name":"functions__sleep","parameters":{"type":"object","properties":{"duration_ms":{"type":"number"}}}}},
        {"name":"collab__exec_command","input_schema":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}},
        {"function_declarations":[{"name":"sleep","parameters":{"type":"object","properties":{"duration_ms":{"type":"number"}}}}]},
        {"functionDeclarations":[{"name":"exec_command","parametersJsonSchema":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}}]}
    ],"input":[{"type":"message"},{"type":"additional_tools","tools":[{"type":"function","name":"functions__exec_command","parameters":{"type":"object","properties":{"yield_time_ms":{"type":["number","null"]}}}}]}],"note":"\u5de5"}"#;

    assert_eq!(
        normalize_codex_tool_integer_types(input.as_bytes(), &BTreeMap::new()),
        input.as_bytes()
    );
    assert_eq!(
        normalize_codex_tool_integer_types(input.as_bytes(), &codex_headers("curl/8.7.1")),
        input.as_bytes()
    );
    assert_eq!(
        normalize_codex_tool_integer_types_for_executor(
            input.as_bytes(),
            &codex_headers("codex-tui/0.154.0"),
            "codex-websockets",
        ),
        input.as_bytes()
    );

    let output = integer_normalize(input, "codex-tui/0.154.0 (Mac OS; arm64)");
    let tools = output["tools"].as_array().unwrap();
    assert_eq!(
        tools[0]["parameters"]["properties"]["cmd"]["type"],
        "string"
    );
    assert_eq!(
        tools[0]["parameters"]["properties"]["yield_time_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[0]["parameters"]["properties"]["timeout_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[1]["parameters"]["properties"]["session_id"]["type"],
        "integer"
    );
    assert_eq!(
        tools[2]["parameters"]["properties"]["duration_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[3]["parameters"]["properties"]["timeout_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[4]["parameters"]["properties"]["max_tokens"]["type"],
        "integer"
    );
    assert_eq!(
        tools[5]["parameters"]["properties"]["limit"]["type"],
        "integer"
    );
    assert_eq!(
        tools[6]["parameters"]["properties"]["participants"]["type"],
        "integer"
    );
    assert_eq!(
        tools[7]["parameters"]["properties"]["timeout_ms"]["type"],
        "number"
    );
    assert_eq!(
        tools[8]["parameters"]["properties"]["duration_ms"]["type"],
        "number"
    );
    assert_eq!(
        tools[9]["function"]["parameters"]["properties"]["duration_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[10]["input_schema"]["properties"]["yield_time_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[11]["function_declarations"][0]["parameters"]["properties"]["duration_ms"]["type"],
        "integer"
    );
    assert_eq!(
        tools[12]["functionDeclarations"][0]["parametersJsonSchema"]["properties"]["yield_time_ms"]
            ["type"],
        "integer"
    );
    assert_eq!(
        output["input"][1]["tools"][0]["parameters"]["properties"]["yield_time_ms"]["type"][0],
        "integer"
    );
    assert_eq!(
        output["input"][1]["tools"][0]["parameters"]["properties"]["yield_time_ms"]["type"][1],
        "null"
    );
    assert_eq!(output["input"][0]["type"], "message");
    assert_eq!(output["note"], "工");
    let raw = normalize_codex_tool_integer_types(
        input.as_bytes(),
        &codex_headers("codex-tui/0.154.0 (Mac OS; arm64)"),
    );
    let raw = std::str::from_utf8(&raw).unwrap();
    assert!(raw.contains("1.00"));
    assert!(raw.contains(r"\u5de5"));

    let deduped = integer_normalize(
        r#"{"tools":[{"type":"function","name":"sleep","parameters":{"type":"object","properties":{"duration_ms":{"type":["number","integer","null"]}}}}]}"#,
        "codex-desktop/0.159.0",
    );
    let types = deduped["tools"][0]["parameters"]["properties"]["duration_ms"]["type"]
        .as_array()
        .unwrap();
    assert_eq!(types.len(), 2);
    assert_eq!(types[0], "integer");
    assert_eq!(types[1], "null");

    let preserved = r#"{"tools":[{"type":"function","name":"custom_schema_tool","parameters":{"type":"object","properties":{"regex_field":{"type":"string","pattern":"\\p{L}+"},"choice_field":{"oneOf":[{"const":"a"},{"const":"b"}]}}}}]}"#;
    assert_eq!(
        normalize_codex_tool_integer_types(preserved.as_bytes(), &codex_headers("codex-tui/1")),
        preserved.as_bytes()
    );
    assert_eq!(
        normalize_codex_tool_schemas(
            br#"{"tools":[{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}}]}"#,
        ),
        br#"{"tools":[{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}}]}"#,
    );
}

#[test]
fn antigravity_request_tools_use_the_same_integer_rules() {
    let input = r#"{"request":{"tools":[{"functionDeclarations":[{"name":"exec_command","parametersJsonSchema":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}}]}]}}"#;
    let output = integer_normalize(input, "Codex Desktop/1.0");
    assert_eq!(
        output["request"]["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]
            ["properties"]["yield_time_ms"]["type"],
        "integer"
    );
    assert_eq!(
        normalize_codex_tool_integer_types(input.as_bytes(), &BTreeMap::new()),
        input.as_bytes()
    );
    assert_eq!(
        normalize_codex_tool_integer_types_for_executor(
            input.as_bytes(),
            &codex_headers("Codex Desktop/1.0"),
            "codex"
        ),
        input.as_bytes()
    );
}
