// ref: internal/runtime/executor/helps/usage_stream_benchmark_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::usage_helpers::StreamUsageBuffer;

const CONTENT: &[u8] = br#"data: {"choices":[{"delta":{"content":"hello"}}]}"#;
const TIER: &[u8] = br#"data: {"service_tier":"default","choices":[]}"#;
const USAGE: &[u8] = br#"data: {"usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30}}"#;

/// Semantic counterpart to the upstream single-content-chunk benchmark: the
/// fast rejection path must leave the already observed tier unchanged.
/// Semantic counterpart to the upstream 100-chunk benchmark: constant-space
/// observation must retain the first tier and final usage, independent of the
/// number of irrelevant content chunks.
#[test]
fn hundred_chunk_stream_finishes_with_terminal_usage() {
    for _ in 0..1_000 {
        let mut buffer = StreamUsageBuffer::default();
        buffer.observe_openai_stream(TIER);
        for _ in 0..98 {
            buffer.observe_openai_stream(CONTENT);
        }
        buffer.observe_openai_stream(USAGE);
        let detail = buffer.detail().unwrap();
        assert_eq!(
            (
                detail.input_tokens,
                detail.output_tokens,
                detail.total_tokens,
                detail.response_service_tier.as_str(),
            ),
            (10, 20, 30, "default")
        );
    }
}
