// ref: internal/util/gemini_schema_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

#[test]
fn candidate_v16_google_schema_properties_force_object_recursively() {
    let schema = json!({
        "properties":{
            "nested":{"type":"string","properties":{"value":{"type":"integer"}}}
        },
        "required":["nested"]
    });
    for clean in [
        clean_json_schema_for_gemini,
        clean_json_schema_for_antigravity,
        clean_json_schema_for_antigravity_response,
    ] {
        let result = clean(&schema);
        assert_eq!(result["type"], "object");
        assert_eq!(result["properties"]["nested"]["type"], "object");
        assert_eq!(
            result["properties"]["nested"]["properties"]["value"]["type"],
            "integer"
        );
        assert_eq!(result["required"], json!(["nested"]));
        assert_eq!(schema["properties"]["nested"]["type"], "string");
    }
}

#[test]
fn candidate_v16_google_schema_properties_name_map_stays_author_owned() {
    let schema = json!({
        "properties":{
            "properties":{"properties":{"type":{"type":"string"}}},
            "type":{"type":"integer"}
        }
    });
    for clean in [
        clean_json_schema_for_gemini,
        clean_json_schema_for_antigravity,
        clean_json_schema_for_antigravity_response,
    ] {
        let result = clean(&schema);
        assert_eq!(result["type"], "object");
        assert_eq!(result["properties"]["properties"]["type"], "object");
        assert_eq!(result["properties"]["type"]["type"], "integer");
        assert!(result["properties"]["properties"]["properties"]
            .get("type")
            .unwrap()
            .is_object());
    }
}

#[test]
fn candidate_v16_google_schema_type_union_prefers_structural_kind() {
    let schema = json!({
        "properties":{
            "record":{"type":["string","object","null"],"properties":{"value":{"type":"string"}}},
            "list":{"type":["string","array"],"items":{"type":"integer"}}
        }
    });
    for clean in [
        clean_json_schema_for_gemini,
        clean_json_schema_for_antigravity,
        clean_json_schema_for_antigravity_response,
    ] {
        let result = clean(&schema);
        assert_eq!(result["properties"]["record"]["type"], "object");
        assert_eq!(result["properties"]["list"]["type"], "array");
        assert_eq!(result["properties"]["list"]["items"]["type"], "integer");
    }
}

#[test]
fn candidate_v16_google_schema_non_object_properties_do_not_force_object() {
    for properties in [Value::Null, json!("invalid"), json!([])] {
        let schema = json!({"type":"string","properties":properties});
        for clean in [
            clean_json_schema_for_gemini,
            clean_json_schema_for_antigravity,
            clean_json_schema_for_antigravity_response,
        ] {
            let result = clean(&schema);
            assert_eq!(result["type"], "string");
            assert_eq!(result["properties"], schema["properties"]);
        }
    }
}

use super::gemini_schema::{
    clean_json_schema_for_antigravity, clean_json_schema_for_antigravity_response,
    clean_json_schema_for_gemini,
};

const PLACEHOLDER_DESCRIPTION: &str = "Brief explanation of why you are calling this tool";

fn parse(input: &str) -> Value {
    serde_json::from_str(input).expect("test fixture must be JSON")
}

#[test]
fn antigravity_converts_const_to_enum() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type": "object",
        "properties": {"kind": {"type": "string", "const": "InsightVizNode"}}
    }));
    assert_eq!(
        cleaned["properties"]["kind"]["enum"],
        json!(["InsightVizNode"])
    );
    assert_eq!(cleaned["properties"]["kind"]["type"], "string");
    assert!(cleaned["properties"]["kind"].get("const").is_none());
}

#[test]
fn antigravity_flattens_nullable_type_and_required() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type": "object",
        "properties": {
            "name": {"type": ["string", "null"]},
            "other": {"type": "string"}
        },
        "required": ["name", "other"]
    }));
    assert_eq!(cleaned["properties"]["name"]["type"], "string");
    assert_eq!(cleaned["properties"]["name"]["description"], "(nullable)");
    assert_eq!(cleaned["required"], json!(["other"]));
}

#[test]
fn constraints_become_description_hints_and_are_removed() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type": "object",
        "properties": {
            "tags": {"type": "array", "description": "List", "minItems": 1, "uniqueItems": true},
            "url": {"type": "string", "format": "uri", "minLength": 3}
        }
    }));
    let tags = &cleaned["properties"]["tags"];
    assert_eq!(
        tags["description"],
        "List (minItems: 1) (uniqueItems: true)"
    );
    assert!(tags.get("minItems").is_none());
    assert!(tags.get("uniqueItems").is_none());
    let url = &cleaned["properties"]["url"];
    assert_eq!(url["description"], "minLength: 3 (format: uri)");
    assert!(url.get("format").is_none());
}

#[test]
fn union_selection_prefers_objects_and_preserves_all_type_hints() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type": "object",
        "properties": {
            "query": {"anyOf": [
                {"type": "null"},
                {"type": "object", "properties": {"kind": {"type": "string"}}}
            ]},
            "label": {"oneOf": [{"type": "string"}, {"type": "integer"}]}
        }
    }));
    let query = &cleaned["properties"]["query"];
    assert_eq!(query["type"], "object");
    assert_eq!(query["description"], "Accepts: null | object");
    assert_eq!(query["properties"]["_"], json!({"type":"boolean"}));
    assert_eq!(query["required"], json!(["_"]));
    assert_eq!(cleaned["properties"]["label"]["type"], "string");
    assert_eq!(
        cleaned["properties"]["label"]["description"],
        "Accepts: string | integer"
    );
}

#[test]
fn union_ties_select_the_first_branch_and_merge_descriptions() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type": "object",
        "properties": {"config": {
            "description": "Parent desc",
            "anyOf": [
                {"type": "string", "description": "First child"},
                {"type": "integer", "description": "Second child"}
            ]
        }}
    }));
    assert_eq!(cleaned["properties"]["config"]["type"], "string");
    assert_eq!(
        cleaned["properties"]["config"]["description"],
        "Parent desc (First child) (Accepts: string | integer)"
    );
}

#[test]
fn all_of_merges_properties_required_and_literal_dot_keys() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type": "object",
        "allOf": [
            {"properties": {"my.param": {"type": "string"}}, "required": ["my.param"]},
            {"properties": {"b": {"type": "integer"}}, "required": ["b"]}
        ]
    }));
    assert_eq!(cleaned["properties"]["my.param"]["type"], "string");
    assert_eq!(cleaned["properties"]["b"]["type"], "integer");
    assert_eq!(cleaned["required"], json!(["my.param", "b"]));
    assert!(cleaned.get("allOf").is_none());
}

#[test]
fn all_of_without_properties_does_not_fabricate_a_properties_map() {
    let cleaned = clean_json_schema_for_antigravity_response(&json!({
        "allOf": [{"description":"only metadata"}]
    }));
    assert!(cleaned.get("properties").is_none());
}

#[test]
fn refs_become_lazy_hints_and_then_receive_empty_object_placeholders() {
    let cleaned = clean_json_schema_for_antigravity(&parse(
        r##"{
          "definitions":{"User":{"type":"object"}},
          "type":"object",
          "properties":{"customer":{
            "description":"He said \"hi\"\\nsecond line",
            "$ref":"#/definitions/User"
          }}
        }"##,
    ));
    let customer = &cleaned["properties"]["customer"];
    assert_eq!(customer["type"], "object");
    assert_eq!(
        customer["description"],
        "He said \"hi\"\\nsecond line (See: User)"
    );
    assert_eq!(customer["required"], json!(["reason"]));
    assert_eq!(
        customer["properties"]["reason"]["description"],
        PLACEHOLDER_DESCRIPTION
    );
    assert!(cleaned.get("definitions").is_none());
}

#[test]
fn a_root_ref_replaces_cyclic_definitions_without_recursing_forever() {
    let cleaned = clean_json_schema_for_antigravity(&parse(
        r##"{
          "definitions":{"Node":{"type":"object","properties":{"child":{"$ref":"#/definitions/Node"}}}},
          "$ref":"#/definitions/Node"
        }"##,
    ));
    assert_eq!(cleaned["type"], "object");
    assert_eq!(cleaned["description"], "See: Node");
    assert_eq!(cleaned["required"], json!(["reason"]));
}

#[test]
fn required_cleanup_keeps_only_literal_property_names() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type":"object",
        "properties":{
            "my.param":{"type":"string"},
            "wild*card?":{"type":"integer"}
        },
        "required":["my.param","wild*card?","missing"]
    }));
    assert_eq!(cleaned["required"], json!(["my.param", "wild*card?"]));
}

#[test]
fn schema_keyword_and_extension_property_names_are_not_removed() {
    let cleaned = clean_json_schema_for_gemini(&json!({
        "$id":"remove-root",
        "x-root":"remove",
        "type":"object",
        "properties":{
            "pattern":{"type":"string","description":"argument"},
            "$id":{"type":"string"},
            "$comment":{"type":"string"},
            "enumDescriptions":{"type":"array"},
            "x-data":{"type":"string"},
            "normal":{"type":"number","x-meta":"remove"}
        },
        "required":["pattern","$id","x-data"]
    }));
    assert!(cleaned.get("$id").is_none());
    assert!(cleaned.get("x-root").is_none());
    for name in ["pattern", "$id", "$comment", "enumDescriptions", "x-data"] {
        assert!(cleaned["properties"].get(name).is_some(), "missing {name}");
    }
    assert!(cleaned["properties"]["normal"].get("x-meta").is_none());
    assert!(cleaned["properties"].get("description").is_none());
}

#[test]
fn enum_hints_and_tool_enum_string_conversion_match_upstream() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type":"object",
        "properties":{
            "priority":{"type":"integer","enum":[0,1,2]},
            "level":{"type":"number","enum":[1.5,2.5,3.5]},
            "enabled":{"type":"boolean","enum":[true,false]},
            "fixed":{"type":"string","enum":["only"]}
        }
    }));
    assert_eq!(cleaned["properties"]["priority"]["type"], "string");
    assert_eq!(
        cleaned["properties"]["priority"]["enum"],
        json!(["0", "1", "2"])
    );
    assert_eq!(
        cleaned["properties"]["level"]["enum"],
        json!(["1.5", "2.5", "3.5"])
    );
    assert_eq!(
        cleaned["properties"]["enabled"]["enum"],
        json!(["true", "false"])
    );
    assert_eq!(
        cleaned["properties"]["priority"]["description"],
        "Allowed: 0, 1, 2"
    );
    assert!(cleaned["properties"]["fixed"].get("description").is_none());
}

#[test]
fn additional_properties_and_multiple_types_add_hints() {
    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type":"object",
        "additionalProperties":false,
        "properties":{"value":{"type":["string","integer","boolean"]}}
    }));
    assert_eq!(cleaned["description"], "No extra properties allowed");
    assert!(cleaned.get("additionalProperties").is_none());
    assert_eq!(
        cleaned["properties"]["value"]["description"],
        "Accepts: string | integer | boolean"
    );
}

#[test]
fn antigravity_adds_empty_and_nested_optional_object_placeholders() {
    let bare = clean_json_schema_for_antigravity(&json!({"type":"object"}));
    assert_eq!(bare["required"], json!(["reason"]));
    assert_eq!(bare["properties"]["reason"]["type"], "string");

    let cleaned = clean_json_schema_for_antigravity(&json!({
        "type":"object",
        "description":"root",
        "properties":{
            "empty":{"type":"object","properties":{}},
            "optional":{"type":"object","properties":{"value":{"type":"string"}}},
            "list":{"type":"array","items":{"type":"object"}}
        }
    }));
    assert_eq!(cleaned["description"], "root");
    assert_eq!(
        cleaned["properties"]["empty"]["required"],
        json!(["reason"])
    );
    assert_eq!(cleaned["properties"]["optional"]["required"], json!(["_"]));
    assert_eq!(
        cleaned["properties"]["list"]["items"]["required"],
        json!(["reason"])
    );
    assert!(cleaned.get("required").is_none());
}

#[test]
fn response_cleaning_preserves_metadata_unions_and_declared_enum_types() {
    let cleaned = clean_json_schema_for_antigravity_response(&json!({
        "type":"object",
        "title":"Response",
        "nullable":true,
        "properties":{
            "empty":{"type":"object"},
            "action":{"anyOf":[
                {"type":"object","properties":{"name":{"type":"string"}},"required":["name"]},
                {"type":"null"}
            ]},
            "label":{"oneOf":[{"type":"string"},{"type":"null"}]},
            "conviction":{"type":"number","enum":[0.25,0.5,1]},
            "count":{"type":"integer","enum":[1,2]}
        }
    }));
    assert_eq!(cleaned["title"], "Response");
    assert_eq!(cleaned["nullable"], true);
    assert!(cleaned["properties"]["empty"].get("properties").is_none());
    assert_eq!(
        cleaned["properties"]["action"]["anyOf"][0]["type"],
        "object"
    );
    assert_eq!(cleaned["properties"]["action"]["anyOf"][1]["type"], "null");
    assert_eq!(cleaned["properties"]["label"]["oneOf"][0]["type"], "string");
    assert_eq!(cleaned["properties"]["conviction"]["type"], "number");
    assert_eq!(
        cleaned["properties"]["conviction"]["enum"],
        json!(["0.25", "0.5", "1"])
    );
    assert_eq!(cleaned["properties"]["count"]["type"], "integer");
}

#[test]
fn gemini_removes_metadata_and_preexisting_tool_placeholders_at_root_and_nested() {
    let schema = json!({
        "$schema":"draft",
        "title":"Root",
        "nullable":true,
        "type":"object",
        "properties":{
            "_":{"type":"boolean"},
            "nested":{"type":"object","title":"Nested","properties":{
                "reason":{"type":"string","description":PLACEHOLDER_DESCRIPTION}
            },"required":["reason"]},
            "payload":{"type":"string","prefill":"hello","deprecated":true}
        },
        "required":["_","nested","payload"]
    });
    let cleaned = clean_json_schema_for_gemini(&schema);
    assert!(cleaned.get("$schema").is_none());
    assert!(cleaned.get("title").is_none());
    assert!(cleaned.get("nullable").is_none());
    assert!(cleaned["properties"].get("_").is_none());
    assert_eq!(cleaned["required"], json!(["nested", "payload"]));
    assert_eq!(cleaned["properties"]["nested"]["properties"], json!({}));
    assert!(cleaned["properties"]["nested"].get("required").is_none());
    assert!(cleaned["properties"]["payload"].get("prefill").is_none());
    assert!(cleaned["properties"]["payload"].get("deprecated").is_none());
}

#[test]
fn cleaning_is_idempotent_for_description_hints() {
    let schema = json!({
        "type":"object",
        "additionalProperties":false,
        "properties":{
            "status":{"type":"string","enum":["active","inactive"]},
            "url":{"type":"string","format":"uri"}
        }
    });
    let once = clean_json_schema_for_antigravity_response(&schema);
    let twice = clean_json_schema_for_antigravity_response(&once);
    assert_eq!(once, twice);
}

#[test]
fn candidate_name_map_parity_distinguishes_property_named_properties() {
    let schema = json!({
        "type":"object",
        "properties":{
            "properties":{
                "type":"object",
                "propertyNames":{"type":"string"},
                "additionalProperties":true
            },
            "records":{
                "type":"array",
                "items":{
                    "type":"object",
                    "properties":{"name":{"type":"string"}},
                    "propertyNames":{"type":"string"}
                }
            }
        }
    });
    for cleaned in [
        clean_json_schema_for_antigravity(&schema),
        clean_json_schema_for_gemini(&schema),
        clean_json_schema_for_antigravity_response(&schema),
    ] {
        assert!(cleaned["properties"]["properties"]
            .get("propertyNames")
            .is_none());
        assert!(cleaned["properties"]["properties"]
            .get("additionalProperties")
            .is_none());
        assert!(cleaned["properties"]["records"]["items"]
            .get("propertyNames")
            .is_none());
    }
}

#[test]
fn candidate_name_map_parity_preserves_author_names_that_match_keywords() {
    let schema = json!({
        "type":"object",
        "properties":{
            "propertyNames":{"type":"string"},
            "patternProperties":{"type":"string"},
            "properties":{
                "type":"object",
                "properties":{"propertyNames":{"type":"string"}}
            }
        }
    });
    for cleaned in [
        clean_json_schema_for_antigravity(&schema),
        clean_json_schema_for_gemini(&schema),
    ] {
        assert!(cleaned["properties"].get("propertyNames").is_some());
        assert!(cleaned["properties"].get("patternProperties").is_some());
        assert!(cleaned["properties"]["properties"]["properties"]
            .get("propertyNames")
            .is_some());
    }
}

#[test]
fn candidate_name_map_parity_covers_defs_and_dependent_schemas() {
    let schema = json!({
        "type":"object",
        "$defs":{
            "properties":{
                "type":"object",
                "propertyNames":{"type":"string"}
            }
        },
        "dependentSchemas":{
            "properties":{
                "type":"object",
                "additionalProperties":false
            }
        }
    });
    let cleaned = clean_json_schema_for_gemini(&schema);
    // `$defs` is unsupported by the private Gemini schema and is removed as
    // a whole after its nested schema has been traversed.
    assert!(cleaned.get("$defs").is_none());
    assert!(cleaned["dependentSchemas"]["properties"]
        .get("additionalProperties")
        .is_none());
}
