// ref: internal/interfaces/client_models.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Content {
    pub role: String,
    pub parts: Option<Vec<Part>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Part {
    #[serde(default, skip_serializing_if = "is_false")]
    pub thought: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(
        rename = "inlineData",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub inline_data: Option<InlineData>,
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub thought_signature: String,
    #[serde(
        rename = "functionCall",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub function_call: Option<FunctionCall>,
    #[serde(
        rename = "functionResponse",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub function_response: Option<FunctionResponse>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineData {
    #[serde(
        rename = "mime_type",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub name: String,
    pub args: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionResponse {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub name: String,
    pub response: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenerateContentRequest {
    #[serde(
        rename = "systemInstruction",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub system_instruction: Option<Content>,
    pub contents: Option<Vec<Content>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDeclaration>,
    #[serde(rename = "generationConfig")]
    pub generation_config: GenerationConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenerationConfig {
    #[serde(rename = "thinkingConfig")]
    pub thinking_config: GenerationConfigThinkingConfig,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub temperature: f64,
    #[serde(rename = "topP", default, skip_serializing_if = "is_zero_f64")]
    pub top_p: f64,
    #[serde(rename = "topK", default, skip_serializing_if = "is_zero_f64")]
    pub top_k: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationConfigThinkingConfig {
    #[serde(rename = "include_thoughts", default, skip_serializing_if = "is_false")]
    pub include_thoughts: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolDeclaration {
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Option<Vec<Value>>,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero_f64(value: &f64) -> bool {
    *value == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

}
