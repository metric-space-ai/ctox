// ref: internal/translator/codex/openai/chat-completions/codex_openai_response_test.go:80-181 @ 16d98881
// Port-Status: candidate
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    convert_codex_response_to_openai_chat_non_stream, convert_codex_response_to_openai_chat_stream,
    CodexToChatStreamState,
};
use serde_json::{json, Value};

fn stream(state: &mut CodexToChatStreamState, event: Value) -> Vec<Value> {
    let raw = format!("data: {event}");
    convert_codex_response_to_openai_chat_stream("gpt", b"{}", b"", raw.as_bytes(), state)
        .into_iter()
        .map(|raw| serde_json::from_slice(&raw).unwrap())
        .collect()
}

fn citation(url: &str, start: i64, end: i64) -> Value {
    json!({"type":"url_citation","url":url,"title":"Example","start_index":start,"end_index":end})
}

#[test]
fn candidate_codex_citation_non_stream_unicode_parts() {
    let raw = json!({"type":"response.completed","response":{
        "id":"citation","model":"gpt","status":"completed","output":[
            {"type":"message","content":[
                {"type":"output_text","text":"前🙂"},
                {"type":"output_text","text":"引用","annotations":[
                    citation("https://example.com",0,2),
                    {"type":"file_citation","file_id":"file"}
                ]}
            ]},
            {"type":"message","content":[
                {"type":"refusal","refusal":"ignored"},
                {"type":"output_text","text":"続","annotations":citation("https://other.example",0,1)}
            ]}
        ]
    }});
    let output: Value = serde_json::from_slice(&convert_codex_response_to_openai_chat_non_stream(
        b"{}",
        b"",
        raw.to_string().as_bytes(),
    ))
    .unwrap();
    let message = &output["choices"][0]["message"];
    assert_eq!(message["content"], "前🙂引用続");
    assert_eq!(message["annotations"].as_array().unwrap().len(), 2);
    assert_eq!(
        message["annotations"][0],
        citation("https://example.com", 2, 4)
    );
    assert_eq!(
        message["annotations"][1],
        citation("https://other.example", 4, 5)
    );
    assert_eq!(output["choices"][0]["finish_reason"], "stop");
}

#[test]
fn candidate_codex_citation_stream_offsets_dedup_and_event_shapes() {
    let mut state = CodexToChatStreamState::default();
    assert_eq!(
        stream(
            &mut state,
            json!({"type":"response.output_text.delta","delta":"前🙂"})
        )
        .len(),
        1
    );
    let event = json!({"type":"response.output_text.annotation.added","annotation":citation("https://example.com",0,1)});
    let output = stream(&mut state, event.clone());
    assert_eq!(output.len(), 1);
    assert_eq!(
        output[0]["choices"][0]["delta"]["annotations"][0],
        citation("https://example.com", 2, 3)
    );
    stream(
        &mut state,
        json!({"type":"response.output_text.delta","delta":"引用"}),
    );
    assert!(stream(
        &mut state,
        json!({"type":"response.output_text.annotation.added",
        "annotation":citation("https://example.com",0,2)})
    )
    .is_empty());
    assert!(stream(&mut state,json!({"type":"response.output_item.done","item":{
        "type":"message","content":[{"type":"output_text","annotations":[citation("https://example.com",0,2)]}]
    }})).is_empty());

    let mut completion = CodexToChatStreamState::default();
    stream(
        &mut completion,
        json!({"type":"response.output_text.delta","delta":"前🙂引用"}),
    );
    let done = stream(
        &mut completion,
        json!({"type":"response.output_text.done","annotations":[citation("https://example.com",0,2)]}),
    );
    assert_eq!(
        done[0]["choices"][0]["delta"]["annotations"][0],
        citation("https://example.com", 4, 6)
    );
    let part = stream(
        &mut completion,
        json!({"type":"response.content_part.done","part":{
            "type":"output_text","annotations":[citation("https://other.example",0,1)]
        }}),
    );
    assert_eq!(
        part[0]["choices"][0]["delta"]["annotations"][0],
        citation("https://other.example", 4, 5)
    );
    let item = stream(
        &mut completion,
        json!({"type":"response.output_item.done","item":{
            "type":"message","annotations":citation("https://item.example",0,1)
        }}),
    );
    assert_eq!(
        item[0]["choices"][0]["delta"]["annotations"][0],
        citation("https://item.example", 4, 5)
    );
    assert!(stream(&mut completion,json!({"type":"response.output_item.done","item":{
        "type":"message","content":[{"type":"output_text","annotations":[citation("https://example.com",0,2)]}]
    }})).is_empty());
    // Deduplication belongs to this response, not a global model/provider cache.
    let mut other = CodexToChatStreamState::default();
    assert_eq!(stream(&mut other, event).len(), 1);
}

#[test]
fn candidate_codex_citation_invalid_ranges_ids_and_empty_metadata() {
    let mut state = CodexToChatStreamState::default();
    let invalid = stream(
        &mut state,
        json!({"type":"response.output_text.done","annotations":[
            citation("https://negative.example",-1,2),
            citation("https://reversed.example",3,2),
            {"type":"file_citation","file_id":"file"},
            null
        ]}),
    );
    assert!(invalid.is_empty());
    let mut first = citation("", 0, 1);
    first["id"] = json!("same-id");
    let mut repeated = citation("https://new-url.example", 2, 4);
    repeated["id"] = json!("same-id");
    assert_eq!(
        stream(
            &mut state,
            json!({"type":"response.output_text.annotation.added","annotation":first})
        )
        .len(),
        1
    );
    assert!(stream(
        &mut state,
        json!({"type":"response.content_part.done","part":{"annotations":repeated}})
    )
    .is_empty());
    assert!(stream(&mut state, json!({"type":"response.output_text.done"})).is_empty());
    stream(
        &mut state,
        json!({"type":"response.output_text.delta","delta":"前🙂"}),
    );
    assert!(stream(
        &mut state,
        json!({"type":"response.output_text.done","annotations":[
            citation("https://overflow.example",i64::MAX,i64::MAX)
        ]})
    )
    .is_empty());
    let raw = json!({"type":"response.completed","response":{"status":"completed","output":[
        {"type":"message","content":[{"type":"output_text","text":"plain","annotations":[]}]}
    ]}});
    let plain: Value = serde_json::from_slice(&convert_codex_response_to_openai_chat_non_stream(
        b"{}",
        b"",
        raw.to_string().as_bytes(),
    ))
    .unwrap();
    assert!(plain["choices"][0]["message"].get("annotations").is_none());
}
