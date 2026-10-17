// ref: examples/plugin/claude-web-search-router/go/stream_forward_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    execute_stream,
    stream_forward::{self, StreamSink},
};
#[derive(Default)]
struct Sink {
    chunks: Vec<Vec<u8>>,
    closed: bool,
}
impl StreamSink for Sink {
    fn emit(&mut self, p: &[u8]) -> Result<(), String> {
        self.chunks.push(p.to_vec());
        Ok(())
    }
    fn close(&mut self, _: Option<&str>) {
        self.closed = true
    }
}
