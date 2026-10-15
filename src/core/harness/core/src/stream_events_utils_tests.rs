use super::handle_non_tool_response_item;
use super::last_assistant_message_from_item;
use super::save_image_generation_result;
use crate::codex::make_session_and_context;
use crate::error::CodexErr;
use ctox_protocol::items::TurnItem;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;

fn assistant_output_text(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: Some("msg-1".to_string()),
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        end_turn: Some(true),
        phase: None,
    }
}


#[test]
fn last_assistant_message_from_item_strips_citations_and_plan_blocks() {
    let item = assistant_output_text(
        "before<oai-mem-citation>doc1</oai-mem-citation>\n<proposed_plan>\n- x\n</proposed_plan>\nafter",
    );

    let message = last_assistant_message_from_item(&item, true)
        .expect("assistant text should remain after stripping");

    assert_eq!(message, "before\nafter");
}


#[test]
fn last_assistant_message_from_item_returns_none_for_plan_only_hidden_message() {
    let item = assistant_output_text("<proposed_plan>\n- x\n</proposed_plan>");

    assert_eq!(last_assistant_message_from_item(&item, true), None);
}
