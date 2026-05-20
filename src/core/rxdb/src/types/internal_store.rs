//! Port of `src/types/rx-database-internal-store.d.ts`.
//!
//! Three shapes for documents that live in the per-database internal store:
//! - [`InternalStoreDocType<Data>`] — generic wrapper.
//! - [`InternalStoreStorageTokenDocType`] — `data` payload for the
//!   storage-token doc.
//! - [`InternalStoreCollectionDocType`] — `data` payload for a per-collection
//!   info doc.
//!
//! Upstream wraps these with `RxDocumentData<...>` (the storage layer's
//! `_rev` / `_meta` / `_attachments` envelope). CTOX serializes the document
//! body as the upstream wire shape; the envelope lives on the storage row
//! itself, so consumers add `_deleted`/`_rev`/etc. when writing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::RxJsonSchema;

// ref: rxdb/src/types/rx-database-internal-store.d.ts:6-11
/// Generic internal-store document. `data` carries the per-context payload
/// (`InternalStoreStorageTokenDocType::Data`, etc.).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InternalStoreDocType {
    pub id: String,
    pub key: String,
    pub context: String,
    pub data: Value,
}

// ref: rxdb/src/types/rx-database-internal-store.d.ts:17-22
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageTokenData {
    #[serde(rename = "rxdbVersion")]
    pub rxdb_version: String,
    pub token: String,
    #[serde(rename = "instanceToken")]
    pub instance_token: String,
    #[serde(
        rename = "passwordHash",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub password_hash: Option<String>,
}

// ref: rxdb/src/types/rx-database-internal-store.d.ts:28-54
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectedStorage {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    pub schema: RxJsonSchema,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectionDocData {
    pub name: String,
    pub schema: RxJsonSchema,
    #[serde(rename = "schemaHash")]
    pub schema_hash: String,
    pub version: u32,
    #[serde(rename = "connectedStorages", default)]
    pub connected_storages: Vec<ConnectedStorage>,
    #[serde(
        rename = "migrationStatus",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub migration_status: Option<Value>,
}

/// Alias for upstream `InternalStoreStorageTokenDocType` — same shape as
/// [`InternalStoreDocType`] but the `data` value is a [`StorageTokenData`].
pub type InternalStoreStorageTokenDocType = InternalStoreDocType;

/// Alias for upstream `InternalStoreCollectionDocType` — same wrapper shape,
/// `data` value is a [`CollectionDocData`].
pub type InternalStoreCollectionDocType = InternalStoreDocType;
