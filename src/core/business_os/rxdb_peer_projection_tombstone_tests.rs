use super::*;

fn chunk_schema() -> RxJsonSchema {
    serde_json::from_value(serde_json::json!({
        "version": 0,
        "primaryKey": "id",
        "type": "object",
        "properties": {
            "id": { "type": "string", "maxLength": 200 },
            "data": { "type": "string" },
            "total": { "type": "integer" },
            "is_deleted": { "type": "boolean" }
        },
        "required": ["id", "data", "total"]
    }))
    .unwrap()
}

#[test]
fn deleted_chunk_with_legacy_omission_marker_gets_schema_valid_data() {
    let schema = chunk_schema();
    let mut document = serde_json::json!({
        "id": "deleted-chunk",
        "data": { "_omitted": true, "_omitted_bytes": 324326,
                  "_omitted_reason": "exceeds peer wire budget" },
        "total": 1,
        "_deleted": true,
        "is_deleted": true
    });
    prepare_projection_tombstone_document(&schema, &mut document);
    assert_eq!(document["data"], "");
    assert_eq!(document["id"], "deleted-chunk");
    assert_eq!(document["total"], 1);
    assert_eq!(document["_deleted"], true);
    assert_eq!(document["is_deleted"], true);
}

#[test]
fn tombstone_preserves_valid_chunk_bytes_and_metadata() {
    let schema = chunk_schema();
    let mut document = serde_json::json!({
        "id": "valid-chunk", "data": "UEsDBAoAAAAAAA==", "total": 4,
        "_deleted": true, "_rev": "3-preserved", "_meta": { "lwt": 123 }
    });
    prepare_projection_tombstone_document(&schema, &mut document);
    assert_eq!(document["data"], "UEsDBAoAAAAAAA==");
    assert_eq!(document["total"], 4);
    assert_eq!(document["_rev"], "3-preserved");
    assert_eq!(document["_meta"]["lwt"], 123);
}

#[test]
fn tombstone_repairs_required_types_and_is_idempotent() {
    let schema = chunk_schema();
    let mut document = serde_json::json!({
        "id": "invalid-chunk", "data": false, "total": "invalid", "_deleted": true
    });
    prepare_projection_tombstone_document(&schema, &mut document);
    assert_eq!(document["data"], "");
    assert_eq!(document["total"], 0);
    let once = document.clone();
    prepare_projection_tombstone_document(&schema, &mut document);
    assert_eq!(document, once);
}

#[test]
fn schema_light_tombstone_still_fills_required_fields() {
    let schema = chunk_schema();
    let mut document = serde_json::json!({ "id": "minimal-chunk", "_deleted": true });
    prepare_projection_tombstone_document(&schema, &mut document);
    assert_eq!(document["data"], "");
    assert_eq!(document["total"], 0);
    assert_eq!(document["is_deleted"], true);
}
