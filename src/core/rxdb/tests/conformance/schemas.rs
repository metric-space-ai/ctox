use rxdb::plugins::utils::utils_string::random_token;
use rxdb::rx_schema_helper::META_LWT_UNIX_TIME_MAX;
use serde_json::{json, Value};

fn string_prop(max_length: Option<u64>) -> Value {
    match max_length {
        Some(max_length) => json!({ "type": "string", "maxLength": max_length }),
        None => json!({ "type": "string" }),
    }
}

fn integer_age() -> Value {
    json!({
        "description": "age in years",
        "type": "integer",
        "minimum": 0,
        "maximum": 150,
        "multipleOf": 1
    })
}

fn base_schema(title: &str, primary_key: Value, properties: Value, required: Value) -> Value {
    json!({
        "title": title,
        "version": 0,
        "keyCompression": false,
        "primaryKey": primary_key,
        "type": "object",
        "properties": properties,
        "required": required
    })
}

fn human_properties() -> Value {
    json!({
        "passportId": string_prop(Some(100)),
        "firstName": string_prop(Some(100)),
        "lastName": string_prop(Some(100)),
        "age": integer_age()
    })
}

pub fn human_schema_literal() -> Value {
    let mut schema = base_schema(
        "human schema",
        json!("passportId"),
        human_properties(),
        json!(["firstName", "lastName", "passportId"]),
    );
    schema["description"] = json!("describes a human being");
    schema["indexes"] = json!(["firstName"]);
    schema
}

pub fn human() -> Value {
    human_schema_literal()
}

pub fn human_default() -> Value {
    let mut schema = base_schema(
        "human schema",
        json!("passportId"),
        human_properties(),
        json!(["passportId"]),
    );
    schema["description"] = json!("describes a human being");
    schema["properties"]["age"]["default"] = json!(20);
    schema["indexes"] = json!([]);
    schema
}

pub fn human_final() -> Value {
    let mut schema = base_schema(
        "human schema with age set final",
        json!("passportId"),
        human_properties(),
        json!(["passportId"]),
    );
    schema["properties"]["age"]["final"] = json!(true);
    schema
}

pub fn simple_human() -> Value {
    base_schema(
        "human schema",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "age": string_prop(Some(100)),
            "oneOptional": string_prop(None)
        }),
        json!(["passportId", "age"]),
    )
}

pub fn simple_human_v3() -> Value {
    let mut schema = simple_human();
    schema["version"] = json!(3);
    schema["properties"]["age"] = json!({
        "type": "number",
        "minimum": 0,
        "maximum": 1000,
        "multipleOf": 1
    });
    schema["indexes"] = json!(["age"]);
    schema
}

pub fn human_age_index() -> Value {
    let mut schema = human_schema_literal();
    schema["indexes"] = json!(["age"]);
    schema["required"] = json!(["firstName", "lastName", "age"]);
    schema
}

pub fn human_sub_index() -> Value {
    let mut schema = base_schema(
        "human schema",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "other": {
                "type": "object",
                "properties": {
                    "age": integer_age()
                }
            }
        }),
        json!(["passportId"]),
    );
    schema["description"] = json!("describes a human being where other.age is index");
    schema["indexes"] = json!(["other.age"]);
    schema
}

pub fn human_with_all_index() -> Value {
    let mut schema = human_schema_literal();
    schema["indexes"] = json!(["firstName", "lastName", "age"]);
    schema["required"] = json!(["firstName", "lastName"]);
    schema
}

pub fn nested_human() -> Value {
    base_schema(
        "human nested",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "firstName": string_prop(Some(100)),
            "mainSkill": {
                "type": "object",
                "properties": {
                    "name": string_prop(Some(10)),
                    "level": {
                        "type": "number",
                        "minimum": 0,
                        "maximum": 10,
                        "multipleOf": 1
                    }
                },
                "required": ["name", "level"],
                "additionalProperties": false
            }
        }),
        json!(["firstName"]),
    )
}

pub fn deep_nested_human() -> Value {
    base_schema(
        "deep human nested",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "mainSkill": {
                "type": "object",
                "properties": {
                    "name": string_prop(None),
                    "attack": {
                        "type": "object",
                        "properties": {
                            "good": { "type": "boolean" },
                            "count": { "type": "number" }
                        }
                    }
                },
                "required": ["name"]
            }
        }),
        json!(["mainSkill"]),
    )
}

pub fn no_index_human() -> Value {
    base_schema(
        "human schema",
        json!("firstName"),
        json!({
            "firstName": string_prop(Some(100)),
            "lastName": string_prop(None)
        }),
        json!(["lastName"]),
    )
}

pub fn no_string_index() -> Value {
    base_schema(
        "no string index",
        json!("passportId"),
        json!({
            "passportId": { "type": "object", "maxLength": 100 },
            "firstName": string_prop(None)
        }),
        json!(["firstName", "passportId"]),
    )
}

pub fn big_human() -> Value {
    let mut schema = base_schema(
        "human schema",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "dnaHash": string_prop(Some(100)),
            "firstName": string_prop(Some(100)),
            "lastName": string_prop(None),
            "age": { "description": "Age in years", "type": "integer", "minimum": 0 }
        }),
        json!(["firstName", "lastName"]),
    );
    schema["description"] = json!("describes a human being with 2 indexes");
    schema["indexes"] = json!(["firstName", "dnaHash"]);
    schema
}

pub fn encrypted_human() -> Value {
    let mut schema = base_schema(
        "human encrypted",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "firstName": string_prop(None),
            "secret": string_prop(None)
        }),
        json!(["firstName", "secret"]),
    );
    schema["encrypted"] = json!(["secret"]);
    schema["indexes"] = json!([]);
    schema
}

pub fn encrypted_object_human() -> Value {
    let mut schema = encrypted_human();
    schema["properties"]["secret"] = json!({
        "type": "object",
        "properties": {
            "name": string_prop(None),
            "subname": string_prop(None)
        }
    });
    schema
}

pub fn encrypted_deep_human() -> Value {
    let mut schema = base_schema(
        "human encrypted",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "firstName": string_prop(None),
            "firstLevelPassword": string_prop(None),
            "secretData": {
                "type": "object",
                "properties": { "pw": string_prop(None) }
            },
            "deepSecret": {
                "type": "object",
                "properties": {
                    "darkhole": {
                        "type": "object",
                        "properties": { "pw": string_prop(None) }
                    }
                }
            },
            "nestedSecret": {
                "type": "object",
                "properties": {
                    "darkhole": {
                        "type": "object",
                        "properties": { "pw": string_prop(None) }
                    }
                }
            }
        }),
        json!(["firstName", "secretData"]),
    );
    schema["encrypted"] = json!([
        "firstLevelPassword",
        "secretData",
        "deepSecret.darkhole.pw",
        "nestedSecret.darkhole.pw"
    ]);
    schema["indexes"] = json!([]);
    schema
}

pub fn not_existing_index() -> Value {
    let mut schema = base_schema(
        "index",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "address": {
                "type": "object",
                "properties": {
                    "street": string_prop(None)
                }
            }
        }),
        json!(["passportId"]),
    );
    schema["indexes"] = json!(["address.apartment"]);
    schema
}

pub fn compound_index() -> Value {
    let mut schema = base_schema(
        "compound index",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "passportCountry": string_prop(Some(100)),
            "age": {
                "type": "integer",
                "minimum": 0,
                "maximum": 150,
                "multipleOf": 1
            }
        }),
        json!(["passportId"]),
    );
    schema["indexes"] = json!([["age", "passportCountry"]]);
    schema
}

pub fn compound_index_no_string() -> Value {
    let mut schema = compound_index();
    schema["properties"]["passportCountry"] = json!({ "type": "object" });
    schema["properties"]["age"] = json!({ "type": "integer" });
    schema["indexes"] = json!([[10, "passportCountry"]]);
    schema
}

pub fn empty() -> Value {
    base_schema(
        "empty schema",
        json!("id"),
        json!({ "id": string_prop(Some(100)) }),
        json!(["id"]),
    )
}

pub fn hero_array() -> Value {
    base_schema(
        "hero schema",
        json!("name"),
        json!({
            "name": string_prop(Some(100)),
            "skills": {
                "type": "array",
                "maxItems": 5,
                "uniqueItems": true,
                "items": {
                    "type": "object",
                    "properties": {
                        "name": string_prop(None),
                        "damage": { "type": "number" }
                    }
                }
            }
        }),
        json!(["name"]),
    )
}

pub fn simple_array_hero() -> Value {
    let mut schema = hero_array();
    schema["properties"]["skills"]["items"] = json!({ "type": "string" });
    schema
}

pub fn primary_human_literal() -> Value {
    let mut schema = human_schema_literal();
    schema["title"] = json!("human schema with primary");
    schema["properties"]["passportId"]["minLength"] = json!(4);
    schema["properties"]["lastName"]["maxLength"] = json!(500);
    schema
}

pub fn primary_human() -> Value {
    primary_human_literal()
}

pub fn human_normalize_schema1_literal() -> Value {
    base_schema(
        "human schema",
        json!("passportId"),
        json!({
            "passportId": {
                "type": "string",
                "minLength": 4,
                "maxLength": 100
            },
            "age": integer_age()
        }),
        json!(["age", "passportId"]),
    )
}

pub fn human_normalize_schema1() -> Value {
    human_normalize_schema1_literal()
}

pub fn human_normalize_schema2() -> Value {
    human_normalize_schema1_literal()
}

pub fn ref_human() -> Value {
    base_schema(
        "human related to other human",
        json!("name"),
        json!({
            "name": string_prop(Some(100)),
            "bestFriend": {
                "ref": "human",
                "type": "string"
            }
        }),
        json!(["name"]),
    )
}

pub fn human_composite_primary() -> Value {
    let mut schema = base_schema(
        "human schema",
        json!({
            "key": "id",
            "fields": ["firstName", "info.age"],
            "separator": "|"
        }),
        json!({
            "id": string_prop(Some(100)),
            "firstName": string_prop(Some(100)),
            "lastName": string_prop(None),
            "info": {
                "type": "object",
                "properties": {
                    "age": {
                        "description": "age in years",
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 150
                    }
                },
                "required": ["age"]
            }
        }),
        json!(["id", "firstName", "lastName", "info"]),
    );
    schema["indexes"] = json!(["firstName"]);
    schema
}

pub fn human_composite_primary_schema_literal() -> Value {
    let mut schema = human_composite_primary();
    schema["encrypted"] = json!([]);
    schema["properties"]["readonlyProps"] = json!({
        "allOf": [],
        "anyOf": [],
        "oneOf": [],
        "type": [],
        "dependencies": { "someDep": ["asd"] },
        "items": [],
        "required": [],
        "enum": []
    });
    schema
}

pub fn ref_human_nested() -> Value {
    base_schema(
        "human related to other human",
        json!("name"),
        json!({
            "name": string_prop(Some(100)),
            "foo": {
                "type": "object",
                "properties": {
                    "bestFriend": {
                        "ref": "human",
                        "type": "string"
                    }
                }
            }
        }),
        json!(["name"]),
    )
}

pub fn average_schema() -> Value {
    let mut schema = base_schema(
        &format!("averageSchema_{}", random_token(Some(5))),
        json!("id"),
        json!({
            "id": { "description": "id", "type": "string", "maxLength": 12 },
            "var1": { "description": "var1", "type": "string", "maxLength": 12 },
            "var2": {
                "description": "var2",
                "type": "number",
                "minimum": 0,
                "maximum": 50000,
                "multipleOf": 1
            },
            "deep": {
                "type": "object",
                "properties": {
                    "deep1": string_prop(Some(10)),
                    "deep2": string_prop(Some(10))
                }
            },
            "list": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "deep1": string_prop(None),
                        "deep2": string_prop(None)
                    }
                }
            }
        }),
        json!(["id", "var1", "var2"]),
    );
    schema["indexes"] = json!(["var1", "var2", "deep.deep1", ["var2", "var1"]]);
    schema["sharding"] = json!({ "shards": 6, "mode": "collection" });
    schema
}

pub fn point() -> Value {
    base_schema(
        "point schema",
        json!("id"),
        json!({
            "id": string_prop(Some(100)),
            "x": { "type": "number" },
            "y": { "type": "number" }
        }),
        json!(["x", "y"]),
    )
}

pub fn human_minimal() -> Value {
    base_schema(
        "human schema",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "age": { "type": "integer" },
            "oneOptional": string_prop(None)
        }),
        json!(["passportId", "age"]),
    )
}

pub fn human_minimal_broken() -> Value {
    base_schema(
        "human schema",
        json!("passportId"),
        json!({
            "passportId": string_prop(Some(100)),
            "broken": { "type": "integer" }
        }),
        json!(["passportId", "broken"]),
    )
}

pub fn human_with_timestamp() -> Value {
    let mut schema = base_schema(
        "human with timestamp",
        json!("id"),
        json!({
            "id": string_prop(Some(100)),
            "name": string_prop(Some(1000)),
            "age": { "type": "number" },
            "updatedAt": {
                "type": "number",
                "minimum": 0,
                "maximum": META_LWT_UNIX_TIME_MAX,
                "multipleOf": 1
            },
            "deletedAt": { "type": "number" }
        }),
        json!(["id", "name", "age", "updatedAt"]),
    );
    schema["indexes"] = json!(["updatedAt"]);
    schema
}

pub fn human_with_timestamp_nested() -> Value {
    let mut schema = human_with_timestamp();
    schema["properties"]["address"] = json!({
        "type": "object",
        "properties": {
            "street": string_prop(None),
            "suite": string_prop(None),
            "city": string_prop(None),
            "zipcode": string_prop(None),
            "geo": {
                "type": "object",
                "properties": {
                    "lat": string_prop(None),
                    "lng": string_prop(None)
                }
            }
        }
    });
    schema
}

pub fn human_with_timestamp_all_index() -> Value {
    let mut schema = human_with_timestamp();
    schema["properties"]["name"]["maxLength"] = json!(100);
    schema["properties"]["age"]["minimum"] = json!(0);
    schema["properties"]["age"]["maximum"] = json!(1500);
    schema["properties"]["age"]["multipleOf"] = json!(1);
    schema["indexes"] = json!(["name", "age", "updatedAt"]);
    schema
}

pub fn human_with_simple_and_compound_indexes() -> Value {
    let mut schema = human_with_timestamp_all_index();
    schema["properties"]["createdAt"] = schema["properties"]["updatedAt"].clone();
    schema["indexes"] = json!([
        ["name", "id"],
        ["age", "id"],
        ["createdAt", "updatedAt", "id"]
    ]);
    schema
}

pub fn human_with_deep_nested_indexes() -> Value {
    let mut schema = base_schema(
        "human with deep nested indexes",
        json!("id"),
        json!({
            "id": string_prop(Some(100)),
            "name": string_prop(Some(100)),
            "job": {
                "type": "object",
                "properties": {
                    "name": string_prop(Some(100)),
                    "manager": {
                        "type": "object",
                        "properties": {
                            "fullName": string_prop(Some(100)),
                            "previousJobs": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "name": string_prop(Some(100))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }),
        json!(["id"]),
    );
    schema["indexes"] = json!(["name", "job.name", "job.manager.fullName"]);
    schema
}

pub fn human_id_and_age_index() -> Value {
    let mut schema = base_schema(
        "human id and age index",
        json!("id"),
        json!({
            "id": string_prop(Some(100)),
            "name": string_prop(None),
            "age": integer_age()
        }),
        json!(["id", "name", "age"]),
    );
    schema["indexes"] = json!([["age", "id"]]);
    schema
}

pub fn human_with_ownership() -> Value {
    let mut schema = human_default();
    schema["properties"]["owner"] = string_prop(Some(128));
    schema
}

pub fn enable_key_compression(schema: &Value) -> Value {
    let mut ret = schema.clone();
    ret["keyCompression"] = json!(true);
    ret
}

#[test]
fn exports_cover_expected_schema_names() {
    let schemas = vec![
        ("human_schema_literal", human_schema_literal()),
        ("human", human()),
        ("human_default", human_default()),
        ("human_final", human_final()),
        ("simple_human", simple_human()),
        ("simple_human_v3", simple_human_v3()),
        ("human_age_index", human_age_index()),
        ("human_sub_index", human_sub_index()),
        ("human_with_all_index", human_with_all_index()),
        ("nested_human", nested_human()),
        ("deep_nested_human", deep_nested_human()),
        ("no_index_human", no_index_human()),
        ("no_string_index", no_string_index()),
        ("big_human", big_human()),
        ("encrypted_human", encrypted_human()),
        ("encrypted_object_human", encrypted_object_human()),
        ("encrypted_deep_human", encrypted_deep_human()),
        ("not_existing_index", not_existing_index()),
        ("compound_index", compound_index()),
        ("compound_index_no_string", compound_index_no_string()),
        ("empty", empty()),
        ("hero_array", hero_array()),
        ("simple_array_hero", simple_array_hero()),
        ("primary_human_literal", primary_human_literal()),
        ("primary_human", primary_human()),
        (
            "human_normalize_schema1_literal",
            human_normalize_schema1_literal(),
        ),
        ("human_normalize_schema1", human_normalize_schema1()),
        ("human_normalize_schema2", human_normalize_schema2()),
        ("ref_human", ref_human()),
        ("human_composite_primary", human_composite_primary()),
        (
            "human_composite_primary_schema_literal",
            human_composite_primary_schema_literal(),
        ),
        ("ref_human_nested", ref_human_nested()),
        ("average_schema", average_schema()),
        ("point", point()),
        ("human_minimal", human_minimal()),
        ("human_minimal_broken", human_minimal_broken()),
        ("human_with_timestamp", human_with_timestamp()),
        ("human_with_timestamp_nested", human_with_timestamp_nested()),
        (
            "human_with_timestamp_all_index",
            human_with_timestamp_all_index(),
        ),
        (
            "human_with_simple_and_compound_indexes",
            human_with_simple_and_compound_indexes(),
        ),
        (
            "human_with_deep_nested_indexes",
            human_with_deep_nested_indexes(),
        ),
        ("human_id_and_age_index", human_id_and_age_index()),
        ("human_with_ownership", human_with_ownership()),
    ];

    assert_eq!(schemas.len(), 43);
    for (name, schema) in schemas {
        assert_eq!(schema["type"], "object", "{name}");
        assert!(matches!(schema["version"].as_i64(), Some(0 | 3)), "{name}");
    }
}

#[test]
fn key_schema_shapes_match_upstream_fixtures() {
    assert_eq!(human()["primaryKey"], "passportId");
    assert_eq!(simple_human_v3()["version"], 3);
    assert_eq!(
        encrypted_deep_human()["encrypted"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        compound_index()["indexes"],
        json!([["age", "passportCountry"]])
    );
    assert_eq!(
        human_composite_primary()["primaryKey"],
        json!({
            "key": "id",
            "fields": ["firstName", "info.age"],
            "separator": "|"
        })
    );
    assert_eq!(
        human_with_deep_nested_indexes()["indexes"],
        json!(["name", "job.name", "job.manager.fullName"])
    );
}

#[test]
fn average_schema_is_randomized_and_key_compression_clones() {
    let left = average_schema();
    let right = average_schema();
    assert_ne!(left["title"], right["title"]);
    assert_eq!(
        left["sharding"],
        json!({ "shards": 6, "mode": "collection" })
    );

    let compressed = enable_key_compression(&left);
    assert_eq!(compressed["keyCompression"], true);
    assert_eq!(left["keyCompression"], false);
}
