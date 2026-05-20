//! In-memory storage backend. Reference implementation used by the
//! conformance harness (gap-item N9) to verify wire-format identity against
//! the upstream JS `storage-memory` plugin.

pub mod binary_search_bounds;
pub mod index_mod;
pub mod memory_helper;
pub mod memory_indexes;
pub mod memory_types;
pub mod rx_storage_instance_memory;

pub use index_mod::{create_storage_instance, get_rx_storage_memory};
