#![allow(unused_imports)]

const UPSTREAM_TEST_UTILS_INDEX: &str =
    include_str!("../../vendor/rxdb-16.20.0/src/plugins/test-utils/index.ts");

pub use crate::config::{
    default_storage, get_config, get_encrypted_storage, get_env_variables, get_password,
    init_test_environment, is_bun, is_deno, is_fast_mode, is_node, is_not_one_of_these_storages,
    set_config, EnvVariables, TestConfig, TestStorageConfig,
};
pub use crate::humans_collection::{
    attachments_schema, create, create_age_index, create_attachments, create_by_schema,
    create_deep_nested, create_human_with_ownership, create_human_with_timestamp,
    create_id_and_age_index, create_migration_collection, create_multi_instance, create_nested,
    create_no_compression, create_primary, create_related, create_related_nested,
    multiple_on_same_db, MultipleCollections,
};
pub use crate::port_manager::next_port;
pub use crate::replication::{
    clean_doc_to_compare, ensure_equal_state, get_pull_handler, get_pull_stream, get_push_handler,
};
pub use crate::revisions::{
    EXAMPLE_REVISION_1, EXAMPLE_REVISION_2, EXAMPLE_REVISION_3, EXAMPLE_REVISION_4,
};
pub use crate::schema_objects::*;
pub use crate::schemas::*;
pub use crate::test_util::{ensure_json_states_equal, repeat_test};

#[test]
fn test_utils_barrel_reexports_ported_modules() {
    assert_eq!(EXAMPLE_REVISION_1, crate::revisions::EXAMPLE_REVISION_1);
    assert_eq!(EXAMPLE_REVISION_2, crate::revisions::EXAMPLE_REVISION_2);
    assert_eq!(EXAMPLE_REVISION_3, crate::revisions::EXAMPLE_REVISION_3);
    assert_eq!(EXAMPLE_REVISION_4, crate::revisions::EXAMPLE_REVISION_4);
    let port = next_port().unwrap();
    assert!(port >= 18_669);
    let mut runs = 0;
    repeat_test(2, |_| runs += 1);
    assert_eq!(runs, 2);
    assert!(ensure_json_states_equal(&[], &[], Some("barrel")).is_ok());
    assert!(is_node());
    assert!(!is_deno());
    assert!(!is_bun());
    assert!(ensure_equal_state(&[], &[], Some("barrel-replication")).is_ok());
    assert!(human_data(None, None, None)["passportId"].is_string());
    assert_eq!(human()["primaryKey"], "passportId");
}

#[test]
fn test_utils_barrel_tracks_vendored_upstream_exports() {
    for upstream_module in [
        "./config.ts",
        "./humans-collection.ts",
        "./port-manager.ts",
        "./revisions.ts",
        "./test-util.ts",
        "./schema-objects.ts",
        "./schemas.ts",
        "./replication.ts",
    ] {
        assert!(
            UPSTREAM_TEST_UTILS_INDEX.contains(upstream_module),
            "vendored test-utils barrel no longer exports {upstream_module}"
        );
    }

    assert!(UPSTREAM_TEST_UTILS_INDEX.contains("export const humansCollection"));
    assert!(UPSTREAM_TEST_UTILS_INDEX.contains("export const schemas"));
    assert!(UPSTREAM_TEST_UTILS_INDEX.contains("export const schemaObjects"));
}
