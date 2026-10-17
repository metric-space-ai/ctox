// ref: internal/logging/request_logger_home_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::request_logger::{FileRequestLogger, RequestLogRecord, RequestLogger};
use super::request_logger_home::{HomeRequestLogPayload, HomeRequestLogSink};
use super::request_logger_writer::DetailedRequestLog;
use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

struct StubHomeRequestSink {
    healthy: bool,
    pushed: Mutex<Vec<HomeRequestLogPayload>>,
}
impl HomeRequestLogSink for StubHomeRequestSink {
    fn heartbeat_ok(&self) -> bool {
        self.healthy
    }
    fn push_request_log(&self, payload: &HomeRequestLogPayload) -> io::Result<()> {
        self.pushed.lock().unwrap().push(payload.clone());
        Ok(())
    }
}

fn record() -> RequestLogRecord {
    RequestLogRecord {
        url: "/v1/chat/completions".into(),
        method: "POST".into(),
        request_headers: BTreeMap::from([
            ("Content-Type".into(), vec!["application/json".into()]),
            ("Authorization".into(), vec!["Bearer secret".into()]),
        ]),
        request_body: br#"{"input":"hello"}"#.to_vec(),
        status_code: 200,
        response_headers: BTreeMap::from([(
            "Content-Type".into(),
            vec!["application/json".into()],
        )]),
        response_body: br#"{"ok":true}"#.to_vec(),
        request_id: "req-1".into(),
        streaming: false,
    }
}
