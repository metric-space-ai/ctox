// ref: internal/util/claude_attribution.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

const CLAUDE_CODE_ATTRIBUTION_SYSTEM_PREFIX: &str = "x-anthropic-billing-header:";

pub fn is_claude_code_attribution_system_text(text: &str) -> bool {
    text.trim_start()
        .starts_with(CLAUDE_CODE_ATTRIBUTION_SYSTEM_PREFIX)
}

/// Removes Claude Code billing/CCH attribution blocks from a Messages body.
///
/// Other system content is kept. Providers such as Kimi and Antigravity may
/// treat this block as prompt text, so callers use this when the active policy
/// has not explicitly opted into a full CLI profile.
#[must_use]
pub fn strip_claude_code_attribution_system(payload: &[u8]) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(payload) else {
        return payload.to_vec();
    };
    let system = gjson::get(document, "system");
    if !system.exists() {
        return payload.to_vec();
    }
    if system.kind() == gjson::Kind::String {
        if !is_claude_code_attribution_system_text(system.str()) {
            return payload.to_vec();
        }
        return delete_system_field(payload);
    }
    if system.kind() != gjson::Kind::Array {
        return payload.to_vec();
    }
    let mut kept = Vec::new();
    let mut removed = false;
    system.each(|_, block| {
        let attribution = block.get("type").str() == "text"
            && is_claude_code_attribution_system_text(block.get("text").str());
        if attribution {
            removed = true;
        } else if !block.json().is_empty() {
            kept.push(block.json().to_owned());
        }
        true
    });
    if !removed {
        return payload.to_vec();
    }
    if kept.is_empty() {
        return delete_system_field(payload);
    }
    let replacement = format!("[{}]", kept.join(","));
    splice_system_value(payload, system.json(), replacement.as_bytes())
}

fn delete_system_field(payload: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<serde_json::Value>(payload) else {
        return payload.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return payload.to_vec();
    };
    object.remove("system");
    serde_json::to_vec(&root).unwrap_or_else(|_| payload.to_vec())
}

fn splice_system_value(payload: &[u8], raw: &str, replacement: &[u8]) -> Vec<u8> {
    let Some(document) = std::str::from_utf8(payload).ok() else {
        return payload.to_vec();
    };
    let document_start = document.as_ptr() as usize;
    let raw_start = raw.as_ptr() as usize;
    if raw_start < document_start {
        return payload.to_vec();
    }
    let start = raw_start - document_start;
    if start > payload.len() || raw.len() > payload.len() - start {
        return payload.to_vec();
    }
    let mut output = Vec::with_capacity(payload.len() - raw.len() + replacement.len());
    output.extend_from_slice(&payload[..start]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&payload[start + raw.len()..]);
    output
}
