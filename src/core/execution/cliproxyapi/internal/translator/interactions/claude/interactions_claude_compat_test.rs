// ref: internal/translator/interactions/claude/interactions_claude_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate; execution pending
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_claude_request_to_interactions, convert_claude_request_to_interactions_with_compat,
};
use serde_json::{json, Value};
