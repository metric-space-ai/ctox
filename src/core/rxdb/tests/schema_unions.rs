use rxdb::rx_schema::validate_write_document;
use rxdb::types::{JsonSchema, RxJsonSchema};
use serde_json::{json, Value};

#[test]
fn nested_type_unions_round_trip_without_changing_the_wire_contract() {
    let value = json!({
        "type": "object",
        "properties": {
            "active_task_id": { "type": ["string", "null"] },
            "entries": { "type": "array", "items": { "type": ["number", "boolean"] } }
        }
    });
    let schema: JsonSchema = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(schema).unwrap(), value);
}

#[test]
fn every_business_os_contract_collection_deserializes_with_union_types_intact() {
    let contract: Value = serde_json::from_str(include_str!(
        "../../business_os/business_os_schema_contract.json"
    ))
    .unwrap();
    for (name, original) in contract.as_object().unwrap() {
        let mut normalized = original.clone();
        if let Some(indexes) = normalized.get_mut("indexes").and_then(Value::as_array_mut) {
            for index in indexes {
                if index.is_string() {
                    *index = json!([index.clone()]);
                }
            }
        }
        let schema: RxJsonSchema =
            serde_json::from_value(normalized).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            serde_json::to_value(schema).unwrap()["properties"],
            original["properties"],
            "{name}"
        );
    }
}

#[test]
fn union_write_validation_accepts_members_and_rejects_other_types() {
    let schema: RxJsonSchema = serde_json::from_value(json!({
        "version": 0, "type": "object", "primaryKey": "id",
        "properties": {
            "id": { "type": "string" },
            "active_task_id": { "type": ["string", "null"], "maxLength": 8 },
            "value": { "type": ["number", "boolean"] }
        },
        "required": ["id", "active_task_id"]
    }))
    .unwrap();
    for value in [json!(null), json!("task")] {
        assert!(validate_write_document(
            &schema,
            "id",
            &json!({
                "id": "one", "active_task_id": value, "value": true
            })
        )
        .is_ok());
    }
    for value in [json!(42), json!({}), json!("too-long-task-id")] {
        assert!(validate_write_document(
            &schema,
            "id",
            &json!({
                "id": "one", "active_task_id": value
            })
        )
        .is_err());
    }
    assert!(validate_write_document(&schema, "id", &json!({"id": "one"})).is_err());
    assert!(validate_write_document(
        &schema,
        "id",
        &json!({
            "id": "one", "active_task_id": null, "value": "wrong"
        })
    )
    .is_err());
    assert!(validate_write_document(
        &schema,
        "id",
        &json!({
            "id": "one", "active_task_id": null, "value": 12
        })
    )
    .is_ok());
}
