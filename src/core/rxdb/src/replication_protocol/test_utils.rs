use std::collections::HashMap;
use std::sync::Arc;

use crate::plugins::storage_memory::get_rx_storage_memory;
use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
use crate::replication_protocol::meta_instance::get_rx_replication_meta_instance_schema;
use crate::rx_schema_helper::fill_with_default_settings;
use crate::types::{
    FirstSyncDone, HashFunction, HashOutput, JsonSchema, PrimaryKey, ReplicationEvents,
    ReplicationStats, RxJsonSchema, RxReplicationHandler, RxStorageInstance,
    RxStorageInstanceCreationParams, RxStorageInstanceReplicationInput,
    RxStorageInstanceReplicationState, StreamQueue, WaitBeforePersist,
};

pub(crate) struct TestHashFunction;

impl HashFunction for TestHashFunction {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { format!("hash:{input}") })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TestSchemaVariant {
    ProtocolIndex,
    Helper,
    Collection,
}

pub(crate) fn test_schema() -> RxJsonSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "id".to_string(),
        JsonSchema {
            schema_type: Some("string".into()),
            max_length: Some(100),
            ..Default::default()
        },
    );
    properties.insert(
        "age".to_string(),
        JsonSchema {
            schema_type: Some("number".into()),
            ..Default::default()
        },
    );
    fill_with_default_settings(RxJsonSchema {
        version: 0,
        primary_key: PrimaryKey::Simple("id".to_string()),
        schema_type: "object".to_string(),
        properties,
        required: vec!["id".to_string()],
        indexes: vec![vec!["age".to_string()]],
        encrypted: Vec::new(),
        internal_indexes: Vec::new(),
        key_compression: false,
        attachments: None,
        additional_properties: true,
        extra: HashMap::new(),
    })
}

pub(crate) fn test_schema_variant(variant: TestSchemaVariant) -> RxJsonSchema {
    match variant {
        TestSchemaVariant::ProtocolIndex => {
            let schema = base_id_schema(true);
            fill_with_default_settings(schema)
        }
        TestSchemaVariant::Helper => RxJsonSchema {
            properties: HashMap::new(),
            required: Vec::new(),
            additional_properties: true,
            ..base_id_schema(true)
        },
        TestSchemaVariant::Collection => base_id_schema(false),
    }
}

fn base_id_schema(additional_properties: bool) -> RxJsonSchema {
    let mut properties = HashMap::new();
    properties.insert(
        "id".to_string(),
        JsonSchema {
            schema_type: Some("string".into()),
            max_length: Some(100),
            ..Default::default()
        },
    );
    RxJsonSchema {
        version: 0,
        primary_key: PrimaryKey::Simple("id".to_string()),
        schema_type: "object".to_string(),
        properties,
        required: vec!["id".to_string()],
        indexes: Vec::new(),
        encrypted: Vec::new(),
        internal_indexes: Vec::new(),
        key_compression: false,
        attachments: None,
        additional_properties,
        extra: HashMap::new(),
    }
}

pub(crate) struct TestReplicationState {
    pub(crate) state: Arc<RxStorageInstanceReplicationState>,
    pub(crate) fork_instance: Arc<dyn RxStorageInstance>,
    pub(crate) meta_instance: Arc<dyn RxStorageInstance>,
}

pub(crate) struct ReplicationStateBuilder {
    database_name: Option<String>,
    schema: RxJsonSchema,
    instances: Option<(Arc<dyn RxStorageInstance>, Arc<dyn RxStorageInstance>)>,
    replication_handler: Arc<dyn RxReplicationHandler>,
    wait_before_persist: Option<WaitBeforePersist>,
    has_attachments: bool,
}

impl ReplicationStateBuilder {
    pub(crate) fn new(
        database_name: impl Into<String>,
        replication_handler: Arc<dyn RxReplicationHandler>,
    ) -> Self {
        Self {
            database_name: Some(database_name.into()),
            schema: test_schema(),
            instances: None,
            replication_handler,
            wait_before_persist: None,
            has_attachments: false,
        }
    }

    pub(crate) fn from_instances(
        fork_instance: Arc<dyn RxStorageInstance>,
        meta_instance: Arc<dyn RxStorageInstance>,
        replication_handler: Arc<dyn RxReplicationHandler>,
    ) -> Self {
        Self {
            database_name: None,
            schema: test_schema(),
            instances: Some((fork_instance, meta_instance)),
            replication_handler,
            wait_before_persist: None,
            has_attachments: false,
        }
    }

    pub(crate) fn schema(mut self, schema: RxJsonSchema) -> Self {
        self.schema = schema;
        self
    }

    pub(crate) fn wait_before_persist(mut self, wait_before_persist: WaitBeforePersist) -> Self {
        self.wait_before_persist = Some(wait_before_persist);
        self
    }

    pub(crate) fn has_attachments(mut self, has_attachments: bool) -> Self {
        self.has_attachments = has_attachments;
        self
    }

    pub(crate) async fn build(self) -> TestReplicationState {
        let (fork_instance, meta_instance) = if let Some(instances) = self.instances {
            instances
        } else {
            let database_name = self
                .database_name
                .expect("database name is required when instances are not supplied");
            let storage = get_rx_storage_memory(());
            let fork_instance: Arc<dyn RxStorageInstance> = storage
                .create_storage_instance(
                    RxStorageInstanceCreationParams {
                        database_instance_token: "db-token".to_string(),
                        database_name: database_name.clone(),
                        collection_name: "docs".to_string(),
                        schema: self.schema.clone(),
                        options: HashMap::new(),
                        multi_instance: false,
                        dev_mode: false,
                        password: None,
                    },
                    (),
                )
                .await
                .unwrap();
            let meta_schema = get_rx_replication_meta_instance_schema(&self.schema, false).unwrap();
            let meta_instance: Arc<dyn RxStorageInstance> = storage
                .create_storage_instance(
                    RxStorageInstanceCreationParams {
                        database_instance_token: "db-token".to_string(),
                        database_name,
                        collection_name: "meta".to_string(),
                        schema: meta_schema,
                        options: HashMap::new(),
                        multi_instance: false,
                        dev_mode: false,
                        password: None,
                    },
                    (),
                )
                .await
                .unwrap();
            (fork_instance, meta_instance)
        };

        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance: Arc::clone(&meta_instance),
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: self.replication_handler,
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: self.wait_before_persist,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: self.has_attachments,
        });

        TestReplicationState {
            state,
            fork_instance,
            meta_instance,
        }
    }
}
