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

}
