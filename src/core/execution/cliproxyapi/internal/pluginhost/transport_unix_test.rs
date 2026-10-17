// Origin: CTOX
// Port-Status: adapted_to_ctox
// Port-Note: Unix LocalTransport isolation evidence
// License: AGPL-3.0-only

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use base64::Engine as _;
use tokio::net::UnixStream;

use super::process_transport::{read_process_message, write_process_message};
use super::rpc_schema::{decode_upstream_json, ProcessMessage};
use super::transport_unix::*;
use crate::sdk::pluginabi::SCHEMA_VERSION;

fn test_root(label: &str) -> PathBuf {
    PathBuf::from("/tmp").join(format!("ctox-plugin-{label}-{}", uuid::Uuid::new_v4()))
}
