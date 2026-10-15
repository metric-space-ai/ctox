// ref: internal/runtime/executor/helps/vertex_payload_helpers_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::borrow::Cow;

use super::strip_vertex_openai_responses_tool_call_ids;
use crate::internal::util::get_gjson_bytes_no_copy;
