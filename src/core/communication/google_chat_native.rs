use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

use crate::communication::adapters::{
    AdapterSyncCommandRequest, ChatSendCommandRequest, ChatTestCommandRequest,
};
use crate::communication::chat_native::{self, ChatPlatform};

pub(crate) fn sync(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<Value> {
    chat_native::sync(ChatPlatform::GoogleChat, root, runtime, request)
}

pub(crate) fn send(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &ChatSendCommandRequest<'_>,
) -> Result<Value> {
    chat_native::send(ChatPlatform::GoogleChat, root, runtime, request)
}

pub(crate) fn test(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &ChatTestCommandRequest<'_>,
) -> Result<Value> {
    chat_native::test(ChatPlatform::GoogleChat, root, runtime, request)
}

pub(crate) fn service_sync(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    chat_native::service_sync(ChatPlatform::GoogleChat, root, settings)
}
