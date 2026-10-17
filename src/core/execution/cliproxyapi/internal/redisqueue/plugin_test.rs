// ref: internal/redisqueue/plugin_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use serde_json::Value;

use crate::internal::logging::{
    set_response_status, with_client_request_metadata, with_endpoint, with_request_id,
    with_response_status_holder, ClientRequestMetadata,
};
use crate::sdk::cliproxy::usage::{
    generate_flag, Detail, Failure, Plugin, Record, TokenAccountingQuality, UsageContext,
    AUTO_SERVICE_TIER, TOKEN_ACCOUNTING_SCHEMA_VERSION,
};

use super::{UsageQueue, UsageQueuePlugin, UsageStatisticsSwitch};

fn fixture() -> (Arc<UsageQueue>, UsageQueuePlugin) {
    let queue = Arc::new(UsageQueue::new());
    queue.set_enabled(true);
    let statistics = Arc::new(UsageStatisticsSwitch::new());
    let plugin = UsageQueuePlugin::new(Arc::clone(&queue), statistics);
    (queue, plugin)
}

fn pop(queue: &UsageQueue) -> Value {
    let items = queue.pop_oldest(10);
    assert_eq!(items.len(), 1);
    serde_json::from_slice(&items[0]).unwrap()
}
