// ref: examples/translator/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use ctox_cliproxyapi::sdk::translator::{
    builtin, gemini, openai, TranslationContext, TranslationState,
};

const MODEL: &str = "gemini-2.5-pro";
const RAW_REQUEST: &[u8] = br#"{"messages":[{"content":[{"text":"Hello! Gemini","type":"text"}],"role":"user"}],"model":"gemini-2.5-pro","stream":false}"#;
const GEMINI_RESPONSE: &[u8] = br#"{"candidates":[{"content":{"role":"model","parts":[{"thought":true,"text":"Okay, here's what's going through my mind. I need to schedule a meeting"},{"thoughtSignature":"","functionCall":{"name":"schedule_meeting","args":{"topic":"Q3 planning","attendees":["Bob","Alice"],"time":"10:00","date":"2025-03-27"}}}]},"finishReason":"STOP","avgLogprobs":-0.50018133435930523}],"usageMetadata":{"promptTokenCount":117,"candidatesTokenCount":28,"totalTokenCount":474,"trafficType":"PROVISIONED_THROUGHPUT","promptTokensDetails":[{"modality":"TEXT","tokenCount":117}],"candidatesTokensDetails":[{"modality":"TEXT","tokenCount":28}],"thoughtsTokenCount":329},"modelVersion":"gemini-2.5-pro","createTime":"2025-08-15T04:12:55.249090Z","responseId":"x7OeaIKaD6CU48APvNXDyA4"}"#;

fn translate_example() -> (bool, Vec<u8>, Vec<u8>) {
    let registry = builtin::registry();
    let context = TranslationContext::default();
    let has_response = registry.has_response_transformer(&openai(), &gemini());
    let translated_request =
        registry.translate_request(&context, &openai(), &gemini(), MODEL, RAW_REQUEST, false);
    let mut state: TranslationState = None;
    let converted_response = registry.translate_non_stream(
        &context,
        &gemini(),
        &openai(),
        MODEL,
        RAW_REQUEST,
        &translated_request,
        GEMINI_RESPONSE,
        &mut state,
    );
    (has_response, translated_request, converted_response)
}

fn main() {
    let (has_response, translated_request, converted_response) = translate_example();
    println!("Has gemini->openai response translator: {has_response}");
    println!(
        "Translated request to Gemini format:\n{}\n",
        String::from_utf8_lossy(&translated_request)
    );
    println!(
        "Converted response for OpenAI clients:\n{}",
        String::from_utf8_lossy(&converted_response)
    );
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
}
