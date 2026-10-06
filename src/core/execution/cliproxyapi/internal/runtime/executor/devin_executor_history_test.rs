// ref: internal/runtime/executor/devin_executor_test.go:144-320,508-580,2600-2702
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_executor_history::{
    parse_devin_interactions_payload, parse_devin_signature_bytes,
};
use super::helps::devin_request::DEVIN_DEFAULT_MAX_TOKENS;
use base64::{engine::general_purpose, Engine as _};

#[test]
fn candidate_devin_history_attaches_thoughts_calls_and_matched_results() {
    let payload = br#"{
        "system_instruction":" You are a helpful coding assistant. ",
        "generation_config":{"temperature":0.8,"max_output_tokens":16000,
            "thinking_level":"high","thinking_config":{"thinking_budget":1024}},
        "previous_interaction_id":"session-uuid-1",
        "input":[
            {"type":"user_input","content":[{"type":"text","text":"hello"}]},
            {"type":"thought","content":[{"type":"text","text":"planning..."}],"signature":" \t\r\n","thought_signature":"c2VhbGVkLnYxLnRlc3Q="},
            {"type":"thought","text":"next thought","signature":"sealed.v1.later"},
            {"type":"model_output","content":[{"type":"text","text":"I can help."}]},
            {"type":"model_output","text":"Next line."},
            {"type":"function_call","name":"read_file","id":" \t","call_id":"call_1","arguments":{ "path":"main.go" }},
            {"type":"function_result","call_id":"call_1","result":"package main\n"}
        ],
        "tools":[{"name":"read_file","description":"Read file","parameters":{"type":"object"}}]
    }"#;
    let prepared = parse_devin_interactions_payload(payload, b"");
    assert_eq!(
        prepared.system_prompt,
        "You are a helpful coding assistant."
    );
    assert_eq!(prepared.temperature, Some(0.8));
    assert_eq!(prepared.max_tokens, 16000);
    assert_eq!(prepared.thinking_level, "high");
    assert_eq!(prepared.budget_tokens, 1024);
    assert_eq!(prepared.session_id, "session-uuid-1");
    assert_eq!(prepared.cascade_id, prepared.session_id);
    assert_eq!(prepared.prompts.len(), 3);
    assert_eq!(prepared.prompts[0].source, 1);
    assert_eq!(prepared.prompts[0].content, "hello");
    let assistant = &prepared.prompts[1];
    assert_eq!(assistant.source, 2);
    assert_eq!(assistant.thinking, "planning...\n\nnext thought");
    assert_eq!(assistant.content, "I can help.\nNext line.");
    assert_eq!(assistant.signature, b"sealed.v1.test");
    assert_eq!(assistant.signature_type, "sealed");
    assert_eq!(assistant.tool_calls.len(), 1);
    assert_eq!(assistant.tool_calls[0].arguments, r#"{ "path":"main.go" }"#);
    assert_eq!(prepared.prompts[2].source, 4);
    assert_eq!(prepared.prompts[2].tool_call_id, "call_1");
    assert_eq!(prepared.prompts[2].content, "package main\n");
    assert!(!prepared.prompts[2].is_orphaned_tool);
    assert_eq!(prepared.tools.len(), 1);
    assert_eq!(prepared.tools[0].parameters, br#"{"type":"object"}"#);
    let ids = prepared
        .prompts
        .iter()
        .map(|item| &item.message_id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 3);
    assert!(ids.iter().all(|id| uuid::Uuid::parse_str(id).is_ok()));
    let debug = format!("{prepared:?}");
    for sensitive in [
        "planning",
        "session-uuid",
        "package main",
        "sealed.v1",
        "main.go",
    ] {
        assert!(!debug.contains(sensitive));
    }
}

#[test]
fn candidate_devin_history_config_session_presence_and_orphan_matching() {
    let zero = parse_devin_interactions_payload(
        br#"{
        "generation_config":{"temperature":0,"max_output_tokens":0},
        "input":[{"type":"thought","text":"a"},{"type":"thought","text":"b"}]
    }"#,
        br#"{"temperature":0.9}"#,
    );
    assert_eq!(zero.temperature, Some(0.0));
    assert_eq!(zero.max_tokens, DEVIN_DEFAULT_MAX_TOKENS);
    assert_eq!(zero.prompts[0].thinking, "a\n\nb");
    let present_null = parse_devin_interactions_payload(
        br#"{
        "generation_config":null,"generationConfig":{"temperature":0.25,"max_output_tokens":8},
        "session_id":" \t\r\n","sessionId":" next-session ",
        "input":[],"messages":[{"role":"user","content":"do-not-use-fallback"}]
    }"#,
        br#"{"temperature":0.75,"conversation_id":" original-session "}"#,
    );
    assert_eq!(present_null.temperature, Some(0.75));
    assert_eq!(present_null.max_tokens, DEVIN_DEFAULT_MAX_TOKENS);
    assert_eq!(present_null.session_id, "next-session");
    assert!(present_null.prompts.is_empty());
    let explicit_null = parse_devin_interactions_payload(
        br#"{"generationConfig":{"temperature":null}}"#,
        br#"{"temperature":0.75}"#,
    );
    assert_eq!(explicit_null.temperature, Some(0.0));
    let fallback = parse_devin_interactions_payload(
        br#"{"temperature":0.1,"sessionId":" stable ","previous_interaction_id":"turn"}"#,
        b"",
    );
    assert_eq!(fallback.temperature, Some(0.1));
    assert_eq!(fallback.session_id, "stable");
    assert_eq!(fallback.cascade_id, "stable");

    let prepared = parse_devin_interactions_payload(
        r#"{
        "messages":[
            {"role":"developer","content":"private-system"},
            {"role":"system","content":"ignored-later-system"},
            {"role":"user","content":"go"},
            {"role":"assistant","content":"calls","tool_calls":[
                {"id":"first","function":{"name":"one","arguments":"{\"q\":\"你好\"}"}},
                {"id":" \t","call_id":"second","name":"two","function":{"name":""},"arguments":{ "n":9007199254740993 }}
            ]},
            {"role":"tool","content":"first result"},
            {"role":"tool","tool_call_id":"second","content":"second result"},
            {"role":"tool","tool_call_id":"second","content":"consumed id is orphan"}
        ]
    }"#
        .as_bytes(),
        b"",
    );
    assert_eq!(prepared.system_prompt, "private-system");
    assert_eq!(prepared.prompts.len(), 5);
    assert_eq!(
        prepared.prompts[1].tool_calls[0].arguments,
        r#"{"q":"你好"}"#
    );
    assert_eq!(
        prepared.prompts[1].tool_calls[1].arguments,
        r#"{ "n":9007199254740993 }"#
    );
    assert_eq!(prepared.prompts[2].source, 4);
    assert_eq!(prepared.prompts[1].tool_calls[0].name, "one");
    assert_eq!(prepared.prompts[1].tool_calls[1].name, "two");
    assert_eq!(prepared.prompts[2].tool_call_id, "first");
    assert_eq!(prepared.prompts[3].tool_call_id, "second");
    assert_eq!(prepared.prompts[4].source, 1);
    assert!(prepared.prompts[4].is_orphaned_tool);
    assert_eq!(prepared.prompts[4].original_tool_call_id, "second");
    assert!(prepared.prompts[4].tool_call_id.is_empty());

    let raw_name = parse_devin_interactions_payload(
        br#"{"messages":[{"role":"assistant","tool_calls":[
            {"id":"first","function":{"name":" \t"},"name":"do-not-replace"},
            {"id":"second","function":{"name":""},"name":"fallback"}
        ]}]}"#,
        b"",
    );
    assert_eq!(raw_name.prompts[0].tool_calls[0].name, " \t");
    assert_eq!(raw_name.prompts[0].tool_calls[1].name, "fallback");
}

#[test]
fn candidate_devin_history_image_sources_mime_priority_and_headers() {
    let prepared = parse_devin_interactions_payload(br#"{"input":[
        {"type":"user_input","content":[
            {"type":"text","text":"transcribe this"},
            {"type":"image","mime_type":"image/jpeg","source":{"data":" A ","media_type":"image/gif"}},
            {"type":"input_image","image_url":{"url":"data:;base64,B"}},
            {"type":"image_url","inline_data":{"data":" C ","mime_type":"image/webp"}},
            {"type":"image_url","url":"https://example.invalid/never-fetch.png"},
            {"type":"image","source":{"data":"D","media_type":"image/gif"}}
        ]},
        {"type":"user_input","content":[
            {"type":"text","text":"[Image user label] keep"},
            {"type":"image","data":"E"}
        ]}
    ]}"#, b"");
    let images = &prepared.prompts[0].images;
    assert_eq!(images.len(), 4);
    assert_eq!(images[0].base64_data, "A");
    assert_eq!(images[0].mime_type, "image/jpeg");
    assert_eq!(images[1].base64_data, "B");
    assert_eq!(images[1].mime_type, "image/png");
    assert_eq!(images[2].base64_data, "C");
    assert_eq!(images[2].mime_type, "image/webp");
    assert_eq!(images[3].base64_data, "D");
    assert_eq!(images[3].mime_type, "image/gif");
    assert_eq!(
        prepared.prompts[0].content,
        concat!(
            "[Image 1: pasted_image_1.jpg]\n[Image 2: pasted_image_2.png]\n",
            "[Image 3: pasted_image_3.webp]\n[Image 4: pasted_image_4.gif]\n\ntranscribe this"
        )
    );
    assert_eq!(prepared.prompts[1].content, "[Image user label] keep");
    assert_eq!(prepared.prompts[1].images.len(), 1);
}

#[test]
fn candidate_devin_history_result_wrappers_do_not_flatten_business_json() {
    for (value, expected) in [
        (
            r#"{"content":{"result":[{"type":"text","text":"wrapped"}]}}"#,
            "wrapped",
        ),
        (
            r#"{"type":"tool_result","tool_use_id":"id","content":"typed","is_error":true}"#,
            "typed",
        ),
        (
            r#"{"result":"business","status":"ok"}"#,
            r#"{"result":"business","status":"ok"}"#,
        ),
        (
            r#"{"type":"text","text":"business","exit_code":1}"#,
            r#"{"type":"text","text":"business","exit_code":1}"#,
        ),
        (
            r#"[{"type":"text","text":"log output"},{ "exit_code":0,"status":"ok" }]"#,
            "log output\n{ \"exit_code\":0,\"status\":\"ok\" }",
        ),
        (
            r#"[{"text":"failed","exit_code":1},"quoted"]"#,
            r#"[{"text":"failed","exit_code":1},"quoted"]"#,
        ),
        ("null", "null"),
        ("\"\"", "{}"),
        ("[]", "[]"),
    ] {
        let payload = format!(r#"{{"input":[{{"type":"function_result","result":{value}}}]}}"#);
        let prepared = parse_devin_interactions_payload(payload.as_bytes(), b"");
        assert_eq!(prepared.prompts[0].content, expected, "{value}");
        assert!(prepared.prompts[0].is_orphaned_tool);
    }
    let missing =
        parse_devin_interactions_payload(br#"{"input":[{"type":"function_result"}]}"#, b"");
    assert_eq!(missing.prompts[0].content, "{}");
    let images = parse_devin_interactions_payload(
        br#"{"input":[{
        "type":"function_result","result":[
            {"type":"text","text":"tool log"},
            {"type":"image","data":"T","mime_type":"image/webp"},
            {"text":"failed","exit_code":1}
        ]
    }]}"#,
        b"",
    );
    assert_eq!(
        images.prompts[0].content,
        "[Image 1: pasted_image_1.webp]\n\ntool log\n{\"text\":\"failed\",\"exit_code\":1}"
    );
    assert_eq!(images.prompts[0].images[0].base64_data, "T");
}

#[test]
fn candidate_devin_history_original_replay_is_owned_and_never_crosses_tool_ids() {
    let mut original = br#"{"messages":[
        {"role":"user","content":[{"type":"image","data":"U1"}]},
        {"role":"assistant","content":[{"type":"thinking","thinking":"original first",
            "signature":"Q0FRU3Rlc3Q="}]},
        {"role":"tool","tool_call_id":"a","content":[{"type":"image","data":"A","mime_type":"image/jpeg"}]},
        {"role":"tool","tool_call_id":"b","content":[{"type":"image","data":"B","mime_type":"image/webp"}]},
        {"role":"tool","tool_call_id":"orphan","content":[{"type":"image","data":"O","mime_type":"image/gif"}]},
        {"role":"user","content":[{"type":"image","data":"U2"}]},
        {"role":"assistant","content":[{"type":"thinking","thinking":"original second",
            "signature":"sealed.v1.replace-nothing"}]}
    ]}"#.to_vec();
    let prepared = parse_devin_interactions_payload(
        br#"{"input":[
        {"type":"user_input","text":"first user"},
        {"type":"function_call","id":"a","name":"one","arguments":{}},
        {"type":"function_call","id":"b","name":"two","arguments":{}},
        {"type":"function_result","call_id":"orphan","result":"orphan result"},
        {"type":"function_result","call_id":"b","result":"b result"},
        {"type":"function_result","call_id":"a","result":"a result"},
        {"type":"user_input","text":"second user"},
        {"type":"model_output","text":"second assistant","signature":"sealed.v1.keep"}
    ]}"#,
        &original,
    );
    original.fill(0);
    assert_eq!(prepared.prompts.len(), 7);
    for (index, data, extension) in [
        (0, "U1", "png"),
        (2, "O", "gif"),
        (3, "B", "webp"),
        (4, "A", "jpg"),
        (5, "U2", "png"),
    ] {
        assert_eq!(prepared.prompts[index].images.len(), 1);
        assert_eq!(prepared.prompts[index].images[0].base64_data, data);
        assert!(prepared.prompts[index]
            .content
            .starts_with(&format!("[Image 1: pasted_image_1.{extension}]")));
    }
    assert!(prepared.prompts[2].is_orphaned_tool);
    assert_eq!(prepared.prompts[2].source, 1);
    assert_eq!(prepared.prompts[3].tool_call_id, "b");
    assert_eq!(prepared.prompts[4].tool_call_id, "a");
    assert_eq!(prepared.prompts[1].signature, b"CAQStest");
    assert_eq!(prepared.prompts[1].signature_type, "anthropic");
    assert_eq!(prepared.prompts[1].thinking, "original first");
    assert_eq!(prepared.prompts[6].signature, b"sealed.v1.keep");
    assert_eq!(prepared.prompts[6].thinking, "original second");
    let mut clone = prepared.clone();
    clone.prompts[0].images[0].base64_data.push_str("-modified");
    clone.prompts[1].signature.fill(0);
    assert_eq!(prepared.prompts[0].images[0].base64_data, "U1");
    assert_eq!(prepared.prompts[1].signature, b"CAQStest");
}

#[test]
fn candidate_devin_history_tools_and_signature_classification_keep_upstream_grammar() {
    let prepared = parse_devin_interactions_payload(br#"{"tools":[
        {"type":"namespace","name":" MCP__CODEX_APP ","tools":null,"children":[
            {"name":" AUTOMATION_UPDATE ","parameters":{}},
            {"name":"exec_command","description":"Run task_id.","parameters":{ "n":9007199254740993,"n":9223372036854775808 }}
        ]},
        {"function_declarations":[{"name":"read_file","parameters":null,"parametersJsonSchema":{"ignored":true}}]},
        {"functionDeclarations":[{"name":"write_stdin","parametersJsonSchema":{"type":"object"}}]},
        {"name":"automation_update","parameters":{}},
        {"name":"","parameters":{}}
    ]}"#, b"");
    assert_eq!(
        prepared
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        [
            "exec_command",
            "read_file",
            "write_stdin",
            "automation_update"
        ]
    );
    // ref: internal/translator/common/devin_tools_test.go:30-99 @ d7914afdedca7af95ee974a42453dc49fc1388ce
    // Upstream changes its exact shell phrases, while custom text stays intact.
    assert_eq!(prepared.tools[0].description, "Run task_id.");
    let known_descriptions = parse_devin_interactions_payload(br#"{"tools":[
        {"name":"exec_command","description":"Runs a command in a bash shell, returning output or a session ID for ongoing interaction.","parameters":{}},
        {"name":"write_stdin","description":"Writes characters to an existing unified exec session and returns recent output.","parameters":{}}
    ]}"#, b"");
    assert_eq!(known_descriptions.tools.len(), 2);
    assert_eq!(known_descriptions.tools[0].description,
        "Runs a command in a bash shell, returning output or an session ID for ongoing interaction.");
    assert_eq!(
        known_descriptions.tools[1].description,
        "Writes characters to a existing unified exec session and returns recent output."
    );
    assert_eq!(
        prepared.tools[0].parameters,
        br#"{ "n":9007199254740993,"n":9223372036854775808 }"#
    );
    assert_eq!(prepared.tools[1].parameters, b"null");
    assert_eq!(prepared.tools[2].parameters, br#"{"type":"object"}"#);
    for (input, bytes, kind) in [
        (" sealed.v1.test ", "sealed.v1.test", "sealed"),
        ("claude#CAQStest12345", "CAQStest12345", "anthropic"),
        ("gpt#gAAAAABk1234567890", "gAAAAABk1234567890", "openai"),
        ("gemini#AY12345", "AY12345", "gemini"),
        ("CAQStest12345", "CAQStest12345", "anthropic"),
        ("gAAAAABk1234567890", "gAAAAABk1234567890", "openai"),
        ("AY12345", "AY12345", "gemini"),
        ("AQID", "AQID", "gemini"),
        ("c2VhbGVkLnYxLnRlc3R=", "sealed.v1.test", "sealed"),
        ("c2Vh\r\nbGVkLnYxLnRlc3Q=", "sealed.v1.test", "sealed"),
        ("Q0FRU3Rlc3Q=", "CAQStest", "anthropic"),
        ("b3BhcXVl", "b3BhcXVl", "sealed"),
        ("", "", ""),
    ] {
        let (parsed, signature_type) = parse_devin_signature_bytes(input);
        assert_eq!(parsed, bytes.as_bytes(), "{input}");
        assert_eq!(signature_type, kind, "{input}");
    }
    // Exercise the shared official-signature detector, not only prefix fallbacks.
    let mut token = vec![0_u8; 1 + 8 + 16 + 16 + 32];
    token[0] = 0x80;
    token[8] = 1;
    for (index, byte) in token.iter_mut().enumerate().skip(9) {
        *byte = index as u8;
    }
    let official = general_purpose::URL_SAFE.encode(token);
    let (parsed, kind) = parse_devin_signature_bytes(&official);
    assert_eq!(kind, "openai");
    assert_eq!(parsed, official.as_bytes());
    let wrapped = general_purpose::STANDARD.encode(official.as_bytes());
    let (parsed, kind) = parse_devin_signature_bytes(&wrapped);
    assert_eq!(kind, "openai");
    assert_eq!(parsed, official.as_bytes());
}
