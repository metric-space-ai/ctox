// ref: internal/runtime/executor/helps/meta_tools.go:10-46 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::translator::common::delete_raw_path;

/// Meta rejects search_content_types on web_search, but accepts it on
/// web_search_preview. Match upstream's one namespace level, preserving all
/// unrelated lexical JSON (including large numbers and duplicate members).
pub fn sanitize_meta_web_search_tools(body: &[u8]) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    let tools = gjson::get(document, "tools");
    if tools.kind() != gjson::Kind::Array {
        return body.to_vec();
    }
    let mut paths = Vec::new();
    for (index, tool) in tools.array().iter().enumerate() {
        if tool.get("type").str() == "web_search" && tool.get("search_content_types").exists() {
            paths.push(format!("tools.{index}.search_content_types"));
        }
        if tool.get("type").str() == "namespace" {
            let nested = tool.get("tools");
            if nested.kind() == gjson::Kind::Array {
                for (subindex, subtool) in nested.array().iter().enumerate() {
                    if subtool.get("type").str() == "web_search"
                        && subtool.get("search_content_types").exists()
                    {
                        paths.push(format!(
                            "tools.{index}.tools.{subindex}.search_content_types"
                        ));
                    }
                }
            }
        }
    }
    paths
        .into_iter()
        .fold(body.to_vec(), |out, path| delete_raw_path(&out, &path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_meta_tools_keep_preview_function_and_raw_siblings() {
        let input = br#"{ "metadata":{"integer":900719925474099312345,"float":1.00e+9},"tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}},{"type":"web_search","external_web_access":true,"search_content_types":["text","image"]},{"type":"web_search_preview","search_content_types":["image"]},{"type":"namespace","name":"search","tools":[{"type":"web_search","search_content_types":null},{"type":"web_search_preview","search_content_types":["text"]}]}]}"#;
        let out = sanitize_meta_web_search_tools(input);
        let document = std::str::from_utf8(&out).unwrap();
        assert!(!gjson::get(document, "tools.1.search_content_types").exists());
        assert!(!gjson::get(document, "tools.3.tools.0.search_content_types").exists());
        assert!(gjson::get(document, "tools.1.external_web_access").bool());
        assert_eq!(gjson::get(document, "tools.0.name").str(), "lookup");
        assert!(gjson::get(document, "tools.2.search_content_types").exists());
        assert!(gjson::get(document, "tools.3.tools.1.search_content_types").exists());
        assert!(document.contains(r#""integer":900719925474099312345,"float":1.00e+9"#));
    }

    #[test]
    fn candidate_meta_tools_noop_preserves_exact_bytes_and_namespace_depth() {
        for input in [
            b"".as_slice(), b"not-json", b"{ \"input\": [] }",
            br#"{"tools":[{"type":"web_search_preview","search_content_types":[]}]}"#,
            br#"{"tools":[{"type":"namespace","tools":[{"type":"namespace","tools":[{"type":"web_search","search_content_types":["text"]}]}]}]}"#,
            br#"{"tools":{"type":"web_search","search_content_types":["text"]}}"#,
        ] { assert_eq!(sanitize_meta_web_search_tools(input),input); }
    }
}
