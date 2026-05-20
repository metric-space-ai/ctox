//! Port of `src/plugins/replication/` — base `RxReplicationState`.

pub mod index_mod;
pub mod replication_helper;

pub use index_mod::{
    default_replication_options, replicate_rx_collection, DocumentModifier, PullHandler,
    PushHandler, ReplicationOptions, ReplicationPullHandlerResult, ReplicationPullOptions,
    ReplicationPushOptions, RxReplicationState, StreamFactory,
};
