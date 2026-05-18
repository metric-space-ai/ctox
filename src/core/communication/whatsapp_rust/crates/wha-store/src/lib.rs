//! Hexagonal store interfaces + an in-memory implementation.
//!
//! The shape mirrors `whatsmeow/store/store.go`: every category of state is
//! its own trait (Identity, Session, PreKey, SenderKey…), and a [`Device`]
//! struct bundles them together via trait objects. Production wiring (sqlite,
//! postgres, ...) plugs in via `Arc<dyn …Store>`.
//!
//! For the foundation port we ship one full impl: [`MemoryStore`] —
//! `Arc<RwLock<…>>` backed, deterministic, sub-millisecond. It is used by
//! every test in the workspace and by the example binaries.

pub mod device;
pub mod error;
pub mod memory;
pub mod persist;
pub mod traits;

pub use device::{AppStateSyncKey, Device, MessageSecretInsert};
pub use error::StoreError;
pub use memory::MemoryStore;
pub use persist::{decode_device, encode_blob, encode_device, DeviceBlob, MAGIC, VERSION};
pub use traits::*;
