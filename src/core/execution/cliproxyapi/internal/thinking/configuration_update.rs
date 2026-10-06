// ref: internal/thinking/configuration_update.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::translator::common::{delete_raw_path, set_raw_path};

use super::{ThinkingConfig, ThinkingLevel, ThinkingMode, LEVEL_AUTO, LEVEL_NONE};

pub(super) fn valid_document(body: &[u8]) -> Option<&str> {
    std::str::from_utf8(body)
        .ok()
        .filter(|document| gjson::valid(document))
}

pub(super) fn is_responses_format(format: &str) -> bool {
    matches!(format, "codex" | "openai-response")
}

/// Ordered, first-member GJSON lookup retains the last nonempty string update.
/// Numbers are validated as JSON syntax, without parsing or narrowing them.
pub(super) fn extract_configuration_update_config(body: &[u8]) -> ThinkingConfig {
    let Some(document) = valid_document(body) else {
        return ThinkingConfig::default();
    };
    let input = gjson::get(document, "input");
    if input.kind() != gjson::Kind::Array {
        return ThinkingConfig::default();
    }
    let mut effort = String::new();
    input.each(|_, item| {
        if item.get("type").str() == "configuration_update" {
            let value = item.get("reasoning.effort");
            if value.kind() == gjson::Kind::String {
                // Go strings.ToLower applies Unicode's simple rune mapping.
                let normalized: String = value
                    .str()
                    .trim()
                    .chars()
                    .map(|ch| ch.to_lowercase().next().unwrap_or(ch))
                    .collect();
                if !normalized.is_empty() {
                    effort = normalized;
                }
            }
        }
        true
    });
    match effort.as_str() {
        "" => ThinkingConfig::default(),
        LEVEL_NONE => ThinkingConfig {
            mode: ThinkingMode::None,
            ..Default::default()
        },
        LEVEL_AUTO => ThinkingConfig {
            mode: ThinkingMode::Auto,
            budget: -1,
            ..Default::default()
        },
        _ => ThinkingConfig {
            mode: ThinkingMode::Level,
            level: ThinkingLevel::new(effort),
            ..Default::default()
        },
    }
}

/// Remove unsupported updates while retaining raw surviving input items/order.
pub(super) fn strip_configuration_updates(body: &[u8]) -> Vec<u8> {
    let Some(document) = valid_document(body) else {
        return body.to_vec();
    };
    let input = gjson::get(document, "input");
    if input.kind() != gjson::Kind::Array {
        return body.to_vec();
    }
    let mut kept = Vec::new();
    let mut removed = false;
    input.each(|_, item| {
        if item.get("type").str() == "configuration_update" {
            removed = true;
        } else {
            kept.push(item.json().to_owned());
        }
        true
    });
    if !removed {
        return body.to_vec();
    }
    set_raw_path(body, "input", format!("[{}]", kept.join(",")).as_bytes())
}

/// Amount removal does not remove summary or unrelated reasoning fields.
pub(super) fn strip_responses_effort(body: &[u8]) -> Vec<u8> {
    let Some(document) = valid_document(body) else {
        return body.to_vec();
    };
    if !gjson::get(document, "reasoning.effort").exists() {
        return body.to_vec();
    }
    let output = delete_raw_path(body, "reasoning.effort");
    let reasoning = crate::internal::util::get_gjson_bytes_no_copy(&output, "reasoning");
    if reasoning.kind() == gjson::Kind::Object {
        let mut has_member = false;
        reasoning.each(|_, _| {
            has_member = true;
            false
        });
        if !has_member {
            return delete_raw_path(&output, "reasoning");
        }
    }
    output
}
