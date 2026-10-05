// ref: internal/runtime/executor/helps/gemini_content_turns.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::borrow::Cow;

use crate::internal::translator::common::set_raw_array_items;

const EMPTY_GEMINI_USER_TURN: &[u8] = br#"{"role":"user","parts":[{"text":""}]}"#;

pub fn ensure_gemini_leading_user_content<'a>(payload: &'a [u8], path: &str) -> Cow<'a, [u8]> {
    ensure_user_content(payload, path, true, false)
}

pub fn ensure_gemini_trailing_user_content<'a>(payload: &'a [u8], path: &str) -> Cow<'a, [u8]> {
    ensure_user_content(payload, path, false, true)
}

pub fn ensure_gemini_boundary_user_content<'a>(payload: &'a [u8], path: &str) -> Cow<'a, [u8]> {
    ensure_user_content(payload, path, true, true)
}

fn ensure_user_content<'a>(
    payload: &'a [u8],
    path: &str,
    leading: bool,
    trailing: bool,
) -> Cow<'a, [u8]> {
    let Ok(document) = std::str::from_utf8(payload) else {
        return Cow::Borrowed(payload);
    };
    // Valid leading-user payloads, including large media, need no item copies.
    let first_role_path = format!("{path}.0.role");
    let first_role = gjson::get(document, &first_role_path);
    if leading && !trailing && first_role.str() != "model" {
        return Cow::Borrowed(payload);
    }
    let contents = gjson::get(document, path);
    if contents.kind() != gjson::Kind::Array {
        return Cow::Borrowed(payload);
    }
    let content_array = contents.array();
    let Some(last_content) = content_array.last() else {
        return Cow::Borrowed(payload);
    };
    let last_role = gjson::get(last_content.json(), "role");
    let parts = gjson::get(last_content.json(), "parts");
    let has_function_response = parts.kind() == gjson::Kind::Array
        && parts
            .array()
            .iter()
            .any(|part| gjson::get(part.json(), "functionResponse").exists());
    let prepend = leading && first_role.str() == "model";
    let append =
        trailing && matches!(last_role.str(), "model" | "assistant") && !has_function_response;
    if !prepend && !append {
        return Cow::Borrowed(payload);
    }
    let mut content_items = Vec::with_capacity(content_array.len() + 2);
    if prepend {
        content_items.push(EMPTY_GEMINI_USER_TURN.to_vec());
    }
    for content in content_array {
        content_items.push(content.json().as_bytes().to_vec());
    }
    if append {
        content_items.push(EMPTY_GEMINI_USER_TURN.to_vec());
    }
    let output = set_raw_array_items(payload, path, &content_items);
    if output == payload {
        Cow::Borrowed(payload)
    } else {
        Cow::Owned(output)
    }
}
