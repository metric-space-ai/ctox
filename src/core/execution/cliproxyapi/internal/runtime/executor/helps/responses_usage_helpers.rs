// ref: internal/runtime/executor/helps/responses_usage_helpers.go:1-108 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: ported — raw JSON and SSE usage detail defaults
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::translator::common::set_raw_path;

/// These are Responses wire defaults; they do not represent measured account limits.
pub fn ensure_responses_usage_details(payload: &[u8]) -> Vec<u8> {
    let trimmed = trim(payload);
    if trimmed.first() == Some(&b'{') {
        let updated = patch_json(trimmed);
        return if updated == trimmed {
            payload.to_vec()
        } else {
            updated
        };
    }
    if !payload.windows(5).any(|v| v == b"data:") {
        return payload.to_vec();
    }
    let mut changed = false;
    let lines: Vec<Vec<u8>> = payload
        .split(|v| *v == b'\n')
        .map(|line| {
            if !trim(line).starts_with(b"data:") {
                return line.to_vec();
            }
            // Match the upstream prefix rule even for an indented data line.
            let prefix = if line.starts_with(b"data: ") { 6 } else { 5 };
            let data = trim(&line[prefix..]);
            if data.first() != Some(&b'{') {
                return line.to_vec();
            }
            let updated = patch_json(data);
            if updated == data {
                return line.to_vec();
            }
            changed = true;
            [line[..prefix].to_vec(), updated].concat()
        })
        .collect();
    if changed {
        lines.join(&b'\n')
    } else {
        payload.to_vec()
    }
}

fn patch_json(payload: &[u8]) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(payload) else {
        return payload.to_vec();
    };
    if gjson::get(document, "object").str() == "response.compaction" {
        return payload.to_vec();
    }
    let mut result = payload.to_vec();
    for path in ["response.usage", "usage"] {
        let usage = gjson::get(document, path);
        if usage.kind() != gjson::Kind::Object {
            continue;
        }
        for (details, field) in [
            ("output_tokens_details", "reasoning_tokens"),
            ("input_tokens_details", "cached_tokens"),
        ] {
            let value = usage.get(details);
            let details_path = format!("{path}.{details}");
            if !value.exists() || value.kind() != gjson::Kind::Object {
                let replacement = format!(r#"{{"{field}":0}}"#);
                result = set_raw_path(&result, &details_path, replacement.as_bytes());
            } else {
                let count = value.get(field);
                if !count.exists() || count.kind() == gjson::Kind::Null {
                    result = set_raw_path(&result, &format!("{details_path}.{field}"), b"0");
                }
            }
        }
    }
    result
}

fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|v| !v.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|v| !v.is_ascii_whitespace())
        .map_or(start, |v| v + 1);
    &bytes[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_meta_usage_details_preserve_measured_values_and_raw_siblings() {
        let payload = br#" { "usage":{"input_tokens":9007199254740993,"input_tokens_details":{"cached_tokens":7},"output_tokens_details":{"reasoning_tokens":null,"other":1.0000000000000001}} } "#;
        let patched = ensure_responses_usage_details(payload);
        let text = std::str::from_utf8(&patched).unwrap();
        assert!(text.contains("9007199254740993"));
        assert!(text.contains("1.0000000000000001"));
        assert_eq!(
            gjson::get(text, "usage.input_tokens_details.cached_tokens").i64(),
            7
        );
        assert_eq!(
            gjson::get(text, "usage.output_tokens_details.reasoning_tokens").i64(),
            0
        );
        assert_eq!(ensure_responses_usage_details(&patched), patched);
        // Upstream sjson creates missing detail objects. Our raw-path setter
        // requires the parent to exist, so exercise both supported usage roots.
        for (payload, path) in [
            (
                br#" {"usage":{"input_tokens":2,"output_tokens":1},"raw":900719925474099312345} "#.as_slice(),
                "usage",
            ),
            (
                br#" {"response":{"usage":{"input_tokens":2,"output_tokens":1}},"raw":900719925474099312345} "#.as_slice(),
                "response.usage",
            ),
        ] {
            let patched = ensure_responses_usage_details(payload);
            let text = std::str::from_utf8(&patched).unwrap();
            assert_eq!(
                gjson::get(text, &format!("{path}.input_tokens_details.cached_tokens")).json(),
                "0"
            );
            assert_eq!(
                gjson::get(text, &format!("{path}.output_tokens_details.reasoning_tokens")).json(),
                "0"
            );
            assert!(text.contains("900719925474099312345"));
            assert_eq!(ensure_responses_usage_details(&patched), patched);
        }
    }
    #[test]
    fn candidate_meta_usage_details_sse_compaction_and_absent_usage_are_exact_noops() {
        for payload in [
            &br#" {"object":"response.compaction","usage":{"input_tokens":3}} "#[..],
            &b"event: completed\ndata: [DONE]\n\n"[..],
            &br#" {"object":"response","output":[]} "#[..],
        ] {
            assert_eq!(ensure_responses_usage_details(payload), payload);
        }
        let payload = b"event: completed\r\ndata: {\"response\":{\"usage\":{\"input_tokens_details\":[],\"output_tokens_details\":null}}}\r\n\r\n";
        let patched = ensure_responses_usage_details(payload);
        assert!(patched.starts_with(b"event: completed\r\ndata: "));
        assert!(patched.ends_with(b"\n\r\n"));
        let line = patched.split(|v| *v == b'\n').nth(1).unwrap();
        let text = std::str::from_utf8(&line[6..]).unwrap();
        assert_eq!(
            gjson::get(text, "response.usage.input_tokens_details.cached_tokens").json(),
            "0"
        );
        assert_eq!(
            gjson::get(
                text,
                "response.usage.output_tokens_details.reasoning_tokens"
            )
            .json(),
            "0"
        );
    }
}
