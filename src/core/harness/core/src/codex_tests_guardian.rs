use super::*;
use crate::features::Feature;
use crate::tools::context::FunctionToolOutput;
use crate::turn_diff_tracker::TurnDiffTracker;
use ctox_protocol::models::NetworkPermissions;
use ctox_protocol::models::PermissionProfile;
use ctox_protocol::models::function_call_output_content_items_to_text;
use pretty_assertions::assert_eq;
use serde::Deserialize;
use std::sync::Arc;

fn expect_text_output(output: &FunctionToolOutput) -> String {
    function_call_output_content_items_to_text(&output.body).unwrap_or_default()
}




#[tokio::test]
#[cfg(unix)]
async fn shell_handler_allows_sticky_turn_permissions_without_inline_request_permissions_feature() {
    let (mut session, turn_context_raw) = make_session_and_context().await;
    session
        .features
        .enable(Feature::RequestPermissionsTool)
        .expect("test setup should allow enabling request permissions tool");
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    {
        let mut active_turn = session.active_turn.lock().await;
        let active_turn = active_turn.as_mut().expect("active turn");
        let mut turn_state = active_turn.turn_state.lock().await;
        turn_state.record_granted_permissions(PermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            ..Default::default()
        });
    }

    let session = Arc::new(session);
    let turn_context = Arc::new(turn_context_raw);

    let handler = ShellHandler;
    let resp = handler
        .handle(ToolInvocation {
            session: Arc::clone(&session),
            turn: Arc::clone(&turn_context),
            tracker: Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new())),
            call_id: "sticky-turn-grant".to_string(),
            tool_name: "shell".to_string(),
            tool_namespace: None,
            payload: ToolPayload::Function {
                arguments: serde_json::json!({
                    "command": [
                        "/bin/sh",
                        "-c",
                        "echo hi",
                    ],
                    "timeout_ms": 1_000_u64,
                    "workdir": Some(turn_context.cwd.to_string_lossy().to_string()),
                })
                .to_string(),
            },
        })
        .await;

    match resp {
        Ok(output) => {
            let output = expect_text_output(&output);

            #[derive(Deserialize, PartialEq, Eq, Debug)]
            struct ResponseExecMetadata {
                exit_code: i32,
            }

            #[derive(Deserialize)]
            struct ResponseExecOutput {
                output: String,
                metadata: ResponseExecMetadata,
            }

            let exec_output: ResponseExecOutput =
                serde_json::from_str(&output).expect("valid exec output json");

            assert_eq!(exec_output.metadata, ResponseExecMetadata { exit_code: 0 });
            assert!(exec_output.output.contains("hi"));
        }
        Err(FunctionCallError::RespondToModel(output)) => {
            assert!(
                !output.contains("additional permissions are disabled"),
                "sticky turn permissions should bypass inline validation: {output}"
            );
        }
        Err(err) => panic!("unexpected error: {err:?}"),
    }
}
