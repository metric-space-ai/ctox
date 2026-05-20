//! Port of `src/plugins/storage-memory/memory-indexes.ts`.

use crate::custom_index::get_indexable_string_monad;
use crate::plugins::storage_memory::memory_types::{
    MemoryStorageInternals, MemoryStorageInternalsByIndex,
};
use crate::rx_error::RxResult;
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::types::RxJsonSchema;

// ref: rxdb/src/plugins/storage-memory/memory-indexes.ts:7-29
pub fn add_indexes_to_internals_state(
    state: &mut MemoryStorageInternals,
    schema: &RxJsonSchema,
) -> RxResult<()> {
    let primary_path = get_primary_field_of_primary_key(&schema.primary_key);
    let mut use_indexes: Vec<Vec<String>> = schema.indexes.clone();

    // we need this index for running cleanup()
    use_indexes.push(vec![
        "_deleted".to_string(),
        "_meta.lwt".to_string(),
        primary_path,
    ]);

    for index_ar in use_indexes.into_iter() {
        let name = get_memory_index_name(&index_ar);
        let get_indexable_string = get_indexable_string_monad(schema, &index_ar)?;
        state.by_index.insert(
            name,
            MemoryStorageInternalsByIndex {
                index: index_ar,
                docs_with_index: Vec::new(),
                get_indexable_string,
            },
        );
    }
    Ok(())
}

// ref: rxdb/src/plugins/storage-memory/memory-indexes.ts:32-34
pub fn get_memory_index_name(index: &[String]) -> String {
    index.join(",")
}
