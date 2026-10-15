// ref: internal/interfaces/types.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

pub type TranslateRequestFunc = crate::sdk::translator::RequestTransform;
pub type TranslateResponseFunc = crate::sdk::translator::ResponseStreamTransform;
pub type TranslateResponseNonStreamFunc = crate::sdk::translator::ResponseNonStreamTransform;
pub type TranslateResponse = crate::sdk::translator::ResponseTransform;

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts_request_alias(_: Option<TranslateRequestFunc>) {}
    fn accepts_stream_alias(_: Option<TranslateResponseFunc>) {}
    fn accepts_non_stream_alias(_: Option<TranslateResponseNonStreamFunc>) {}
    fn accepts_response_alias(_: Option<TranslateResponse>) {}

}
