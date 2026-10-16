//! Fixture admission only; production command ordering/fence/projection/subscription.
use ctox_store::{
    change_log::{
        self,
        subscription::{
            protocol::*,
            write_contract::{self, OrderedWriter},
            ReadAuthority, Subscription, SubscriptionKey, WriteAuthority, WritePermission,
        },
    },
    read_pool::ReadPool,
    registry::Registry,
    schema::{Collection, CollectionDefinition},
    writer::{WriterHandle, WriterOptions},
    DbId, JobOptions, WriteJob,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use rusqlite::Transaction;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
pub struct Policy;
impl WriteAuthority for Policy {
    fn authorize(
        &self,
        _: &Transaction<'_>,
        p: &str,
        _: &Write,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if p.starts_with("client-") || p.starts_with("executor-") {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &Collection,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Permit)
    }
    fn authorize_command_intent(
        &self,
        _: &Transaction<'_>,
        p: &str,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if p.starts_with("client-") {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
    fn authorize_command_executor(
        &self,
        _: &Transaction<'_>,
        p: &str,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if p.starts_with("executor-") {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
}
impl ReadAuthority for Policy {
    fn authorize(&self, _: &Transaction<'_>, k: &SubscriptionKey) -> rusqlite::Result<()> {
        if k.scope_hash != "m8-fixture-rights-1" {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    }
    fn project(
        &self,
        _: &Transaction<'_>,
        _: &SubscriptionKey,
        d: &change_log::ChangeDocument,
    ) -> rusqlite::Result<Option<Value>> {
        Ok(d.body.clone())
    }
}
pub struct Signer(Ed25519KeyPair);
impl Signer {
    pub fn new() -> Self {
        // Public deterministic TEST key; never an instance credential.
        Self(Ed25519KeyPair::from_seed_unchecked(&[17; 32]).unwrap())
    }
    pub fn public(&self) -> Vec<u8> {
        self.0.public_key().as_ref().to_vec()
    }
}
impl InstanceSigner for Signer {
    fn instance_id(&self) -> &str {
        "m8-component"
    }
    fn key_id(&self) -> &str {
        "public-test-17"
    }
    fn sign(&self, b: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign(b).as_ref().to_vec())
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn op() -> String {
    let n = now();
    let mut bytes = *uuid::Uuid::new_v4().as_bytes();
    bytes[..6].copy_from_slice(&n.to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 15) | 0x70;
    uuid::Uuid::from_bytes(bytes).to_string()
}
pub fn request(client: usize, seq: u64) -> OrderedWrite {
    let id = format!("m8-{client}-{seq}");
    OrderedWrite {
        client_seq: Seq(seq),
        write: Write {
            op_id: op(),
            prev_op_id: None,
            collection: "business_commands".into(),
            id: id.clone(),
            base_rev: None,
            base_seq: None,
            gen: 1,
            schema_ver: 3,
            app_ver: "platform-7".into(),
            op: WriteOp::Put,
            body: Some(
                json!({"id":id,"app_id":"fixture","type":"task","payload":{"value":seq},"created_at":now(),"status":"queued"}),
            ),
        },
    }
}
pub struct Driver {
    writer: WriterHandle,
    readers: ReadPool,
    ingress: OrderedWriter,
    live: Vec<Subscription>,
    signer: Signer,
    executor_seq: u64,
    pub accepted: u64,
    pub terminal: u64,
    pub projection_reads: u64,
}
impl Driver {
    pub async fn open(path: std::path::PathBuf) -> Self {
        // StoreFixture uses WriterOptions::default(), which deliberately has no
        // Sync head hook. Install the real _seq hook so commits wake readers.
        let writer = Registry
            .open(
                DbId::new(path).unwrap(),
                WriterOptions {
                    head_seq: Some(Arc::new(|connection| {
                        Ok(connection.query_row(
                            "SELECT v FROM _seq WHERE singleton=1",
                            [],
                            |row| row.get(0),
                        )?)
                    })),
                    ..Default::default()
                },
            )
            .unwrap();
        writer.ready().await.unwrap();
        writer
            .submit(WriteJob::new(JobOptions::default(), |tx| {
                ctox_store::schema::create_collection(
                    tx,
                    &CollectionDefinition {
                        name: "business_commands".into(),
                        app_id: "ctox-platform".into(),
                        schema_ver: 3,
                        schema_json: write_contract::command_document_schema(),
                        class: "full".into(),
                        client_limit: 10000,
                        byte_budget: 64 * 1024 * 1024,
                        rights_ver: 1,
                        app_ver: "platform-7".into(),
                    },
                )?;
                write_contract::activate_command_projections(tx, 1)?;
                Ok(())
            }))
            .unwrap()
            .await
            .unwrap();
        let readers = ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            Default::default(),
            Default::default(),
        )
        .await
        .unwrap();
        let signer = Signer::new();
        let mut live = Vec::new();
        for client in 0..2 {
            let mut subscription = Subscription::new(
                readers.clone(),
                &writer,
                SubscriptionKey {
                    participant: format!("client-{client}"),
                    collection: "business_commands".into(),
                    scope_hash: "m8-fixture-rights-1".into(),
                    gen: 1,
                },
                None,
            )
            .unwrap();
            subscription
                .next_page(100, Arc::new(Policy), &signer, now)
                .await
                .unwrap();
            live.push(subscription);
        }
        Self {
            ingress: OrderedWriter::new(writer.clone(), Arc::new(Policy)),
            writer,
            readers,
            live,
            signer,
            executor_seq: 0,
            accepted: 0,
            terminal: 0,
            projection_reads: 0,
        }
    }
    pub async fn execute(&mut self, client: usize, wire: Value) -> Value {
        let ClientMessage::Write(w) = serde_json::from_value(wire).unwrap() else {
            panic!("not a V4 Write")
        };
        let id = w.write.id.clone();
        let mut document = w.write.body.clone().unwrap();
        let result = self
            .ingress
            .submit(format!("client-{client}"), w, now())
            .await
            .unwrap();
        assert_eq!(result.results.len(), 1);
        let receipt = &result.results[0].result;
        assert_eq!(receipt.status, WriteStatus::Accepted);
        self.accepted += 1;
        let executor = "executor-component";
        let lease = self
            .ingress
            .claim_command(executor.into(), id.clone(), None, now())
            .await
            .unwrap()
            .unwrap();
        document["status"] = json!("done");
        document["executor_fence"] = json!(lease.fence);
        document["result"] = json!({"value":document["payload"]["value"]});
        self.executor_seq += 1;
        let terminal = OrderedWrite {
            client_seq: Seq(self.executor_seq),
            write: Write {
                op_id: op(),
                prev_op_id: None,
                collection: "business_commands".into(),
                id: id.clone(),
                base_rev: receipt.rev.clone(),
                base_seq: receipt.seq,
                gen: 1,
                schema_ver: 3,
                app_ver: "platform-7".into(),
                op: WriteOp::Put,
                body: Some(document),
            },
        };
        let done = self
            .ingress
            .submit(executor.into(), terminal, now())
            .await
            .unwrap();
        assert_eq!(done.results[0].result.status, WriteStatus::Accepted);
        self.terminal += 1;
        let commit_ns = crate::elapsed_ns();
        let check = self
            .readers
            .read(move |tx| write_contract::read_command_projection(tx, &id))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(check.status, "completed");
        self.projection_reads += 1;
        let mut deliveries = Vec::new();
        for (target, subscription) in self.live.iter_mut().enumerate() {
            let page = subscription
                .next_page(100, Arc::new(Policy), &self.signer, now)
                .await
                .unwrap();
            deliveries.push(json!({"kind":"live","target":target,"page":page,"commit_ns":commit_ns,"public_key":self.signer.public()}));
        }
        json!({"type":"component_command_result","receipt":{"results":result.results},"terminal":{"results":done.results},"deliveries":deliveries})
    }
    pub async fn shutdown(&self) {
        self.writer.stop_admission();
        self.readers.shutdown().await.unwrap();
        self.writer.shutdown().await.unwrap();
    }
}
