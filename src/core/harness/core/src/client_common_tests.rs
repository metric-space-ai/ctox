use ctox_api::ResponsesApiRequest;
use ctox_api::common::OpenAiVerbosity;
use ctox_api::common::TextControls;
use ctox_api::create_text_param_for_request;
use ctox_protocol::config_types::ServiceTier;
use ctox_protocol::models::FunctionCallOutputPayload;
use pretty_assertions::assert_eq;

use super::*;





#[test]
fn reserializes_shell_outputs_for_function_and_custom_tool_calls() {
    let raw_output = r#"{"output":"hello","metadata":{"exit_code":0,"duration_seconds":0.5}}"#;
    let expected_output = "Exit code: 0\nWall time: 0.5 seconds\nOutput:\nhello";
    let mut items = vec![
        ResponseItem::FunctionCall {
            id: None,
            name: "shell".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-1".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_text(raw_output.to_string()),
        },
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "call-2".to_string(),
            name: "apply_patch".to_string(),
            input: "*** Begin Patch".to_string(),
        },
        ResponseItem::CustomToolCallOutput {
            call_id: "call-2".to_string(),
            output: FunctionCallOutputPayload::from_text(raw_output.to_string()),
        },
    ];

    reserialize_shell_outputs(&mut items);

    assert_eq!(
        items,
        vec![
            ResponseItem::FunctionCall {
                id: None,
                name: "shell".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "call-1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: FunctionCallOutputPayload::from_text(expected_output.to_string()),
            },
            ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "call-2".to_string(),
                name: "apply_patch".to_string(),
                input: "*** Begin Patch".to_string(),
            },
            ResponseItem::CustomToolCallOutput {
                call_id: "call-2".to_string(),
                output: FunctionCallOutputPayload::from_text(expected_output.to_string()),
            },
        ]
    );
}
