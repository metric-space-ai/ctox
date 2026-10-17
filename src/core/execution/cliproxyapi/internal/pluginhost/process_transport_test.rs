// Origin: CTOX
// Port-Status: adapted_to_ctox
// Port-Note: supplemental process-frame transport evidence
// License: AGPL-3.0-only

use serde_json::json;
use tokio::io::{duplex, AsyncWriteExt};

use super::process_transport::*;
use super::rpc_schema::{encode_upstream_json, ProcessMessage, PROCESS_PROTOCOL_VERSION};
use crate::sdk::pluginabi::Envelope;

fn cancel(request_id: &str) -> ProcessMessage {
    ProcessMessage::Cancel {
        protocol_version: PROCESS_PROTOCOL_VERSION,
        request_id: request_id.into(),
    }
}
