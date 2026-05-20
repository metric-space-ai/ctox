use std::time::{SystemTime, UNIX_EPOCH};

use rxdb::plugins::utils::utils_number::random_number;
use serde_json::{json, Map, Value};

pub const TEST_DATA_CHARSET: &str =
    "0987654321ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyzäöüÖÄßÜ[]{}'";
const SOME_EMOJIS: &[&str] = &["😊", "💩", "👵", "🍌", "🏳️‍🌈", "😃"];

pub fn test_data_charset_last_sorted() -> char {
    let mut chars: Vec<char> = TEST_DATA_CHARSET.chars().collect();
    chars.sort_unstable();
    *chars.last().expect("test charset is not empty")
}

fn random_usize(max_inclusive: usize) -> usize {
    random_number(Some(0), Some(max_inclusive as i64)) as usize
}

fn random_string(min_length: usize, max_length: usize) -> String {
    random_string_with_special_chars(min_length, max_length).expect("valid random string length")
}

pub fn random_string_with_special_chars(
    min_length: usize,
    max_length: usize,
) -> Result<String, String> {
    if min_length == 0 || max_length == 0 || min_length > max_length {
        return Err(format!("invalid length given {min_length} {max_length}"));
    }

    let base_chars: Vec<String> = TEST_DATA_CHARSET
        .chars()
        .map(|character| character.to_string())
        .collect();
    let mut all_chars = base_chars.clone();
    all_chars.extend(SOME_EMOJIS.iter().map(|emoji| (*emoji).to_string()));
    let length = random_number(Some(min_length as i64), Some(max_length as i64)) as usize;

    loop {
        let mut text = String::new();
        while text.chars().count() < length {
            let next = if text.is_empty() {
                &base_chars[random_usize(base_chars.len() - 1)]
            } else {
                &all_chars[random_usize(all_chars.len() - 1)]
            };
            text.push_str(next);
        }

        if text.chars().count() == length {
            return Ok(text);
        }
    }
}

fn merge_partial(mut base: Value, partial: Option<Value>) -> Value {
    if let (Value::Object(base_object), Some(Value::Object(partial_object))) = (&mut base, partial)
    {
        for (key, value) in partial_object {
            base_object.insert(key, value);
        }
    }
    base
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis() as i64
}

pub fn human_data(passport_id: Option<&str>, age: Option<i64>, first_name: Option<&str>) -> Value {
    json!({
        "passportId": passport_id.map(ToOwned::to_owned).unwrap_or_else(|| random_string(8, 12)),
        "firstName": first_name.map(ToOwned::to_owned).unwrap_or_else(|| random_string(8, 12)),
        "lastName": random_string(8, 12),
        "age": age.unwrap_or_else(|| random_number(Some(10), Some(50)))
    })
}

pub fn simple_human_data() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "firstName": random_string(8, 12),
        "lastName": random_string(8, 12)
    })
}

pub fn simple_human_v3_data(partial: Option<Value>) -> Value {
    merge_partial(
        json!({
            "passportId": random_string(8, 12),
            "age": random_number(Some(10), Some(50))
        }),
        partial,
    )
}

pub fn simple_human_age(partial: Option<Value>) -> Value {
    merge_partial(
        json!({
            "passportId": random_string(8, 12),
            "age": random_number(Some(10), Some(50)).to_string()
        }),
        partial,
    )
}

pub fn human_with_sub_other() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "other": {
            "age": random_number(Some(10), Some(50))
        }
    })
}

pub fn no_index_human() -> Value {
    json!({
        "firstName": random_string(8, 12),
        "lastName": random_string(8, 12)
    })
}

pub fn nested_human_data(partial: Option<Value>) -> Value {
    merge_partial(
        json!({
            "passportId": random_string(8, 12),
            "firstName": random_string(8, 12),
            "mainSkill": {
                "name": random_string(4, 6),
                "level": 5
            }
        }),
        partial,
    )
}

pub fn deep_nested_human_data() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "mainSkill": {
            "name": random_string(4, 6),
            "attack": {
                "good": false,
                "count": 5
            }
        }
    })
}

pub fn big_human_document_type() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "dnaHash": random_string(8, 12),
        "firstName": random_string(8, 12),
        "lastName": random_string(8, 12),
        "age": random_number(Some(10), Some(50))
    })
}

pub fn hero_array_data() -> Value {
    json!({
        "name": random_string(6, 8),
        "skills": (0..3)
            .map(|_| json!({
                "name": random_string(4, 6),
                "damage": random_number(Some(10), Some(50))
            }))
            .collect::<Vec<_>>()
    })
}

pub fn simple_hero_array(partial: Option<Value>) -> Value {
    merge_partial(
        json!({
            "name": random_string(6, 8),
            "skills": (0..3)
                .map(|_| json!(random_string(3, 6)))
                .collect::<Vec<_>>()
        }),
        partial,
    )
}

pub fn encrypted_human_data(secret: Option<&str>) -> Value {
    json!({
        "passportId": random_string(8, 12),
        "firstName": random_string(8, 12),
        "secret": secret.map(ToOwned::to_owned).unwrap_or_else(|| random_string(8, 12))
    })
}

pub fn encrypted_object_human_data() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "firstName": random_string(8, 12),
        "secret": {
            "name": random_string(8, 12),
            "subname": random_string(8, 12)
        }
    })
}

pub fn encrypted_deep_human_document_type() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "firstName": random_string(8, 12),
        "firstLevelPassword": random_string(8, 12),
        "secretData": {
            "pw": random_string(8, 12)
        },
        "deepSecret": {
            "darkhole": {
                "pw": random_string(8, 12)
            }
        },
        "nestedSecret": {
            "darkhole": {
                "pw": random_string(8, 12)
            }
        }
    })
}

pub fn compound_index_data() -> Value {
    json!({
        "passportId": random_string(8, 12),
        "passportCountry": random_string(8, 12),
        "age": random_number(Some(10), Some(50))
    })
}

pub fn compound_index_no_string_data() -> Value {
    let mut country = Map::new();
    country.insert(random_string(8, 12), json!(random_string(8, 12)));
    json!({
        "passportId": random_string(8, 12),
        "passportCountry": country,
        "age": random_number(Some(10), Some(50))
    })
}

pub fn nostring_index() -> Value {
    json!({
        "passportId": {},
        "firstName": random_string(8, 12)
    })
}

pub fn ref_human_data(best_friend: Option<&str>) -> Value {
    let mut ret = Map::new();
    ret.insert("name".to_string(), json!(random_string(8, 12)));
    if let Some(best_friend) = best_friend {
        ret.insert("bestFriend".to_string(), json!(best_friend));
    }
    Value::Object(ret)
}

pub fn ref_human_nested_data(best_friend: Option<&str>) -> Value {
    let mut foo = Map::new();
    if let Some(best_friend) = best_friend {
        foo.insert("bestFriend".to_string(), json!(best_friend));
    }
    json!({
        "name": random_string(8, 12),
        "foo": foo
    })
}

pub fn human_with_timestamp_data(given_data: Option<Value>) -> Value {
    merge_partial(
        json!({
            "id": random_string(8, 12),
            "name": random_string(8, 12),
            "age": random_number(Some(1), Some(100)),
            "updatedAt": now_millis()
        }),
        given_data,
    )
}

pub fn average_schema_data(partial: Option<Value>) -> Value {
    merge_partial(
        json!({
            "id": random_string(11, 12),
            "var1": random_string(9, 12),
            "var2": random_number(Some(100), Some(50_000)),
            "deep": {
                "deep1": random_string(7, 10),
                "deep2": random_string(7, 10),
                "deeper": {
                    "deepNr": random_number(Some(0), Some(10))
                }
            },
            "list": (0..5)
                .map(|_| json!({
                    "deep1": random_string(2, 5),
                    "deep2": random_string(5, 8)
                }))
                .collect::<Vec<_>>()
        }),
        partial,
    )
}

pub fn point_data() -> Value {
    json!({
        "id": random_string(8, 12),
        "x": random_number(Some(1), Some(100)),
        "y": random_number(Some(1), Some(100))
    })
}

pub fn human_with_id_and_age_index_document_type(age: Option<i64>) -> Value {
    json!({
        "id": random_string(8, 12),
        "name": random_string(8, 12),
        "age": age.unwrap_or_else(|| random_number(Some(1), Some(100)))
    })
}

pub fn human_with_composite_primary(partial: Option<Value>) -> Value {
    merge_partial(
        json!({
            "firstName": random_string(8, 12),
            "lastName": random_string(8, 12),
            "info": {
                "age": random_number(Some(10), Some(50))
            }
        }),
        partial,
    )
}

pub fn human_with_ownership_data(partial: Option<Value>, owner: &str) -> Value {
    merge_partial(
        json!({
            "passportId": random_string(8, 12),
            "firstName": random_string(8, 12),
            "lastName": random_string(8, 12),
            "age": random_number(Some(10), Some(50)),
            "owner": owner
        }),
        partial,
    )
}

#[test]
fn random_string_uses_special_charset_and_validates_lengths() {
    assert!(random_string_with_special_chars(0, 1).is_err());
    let value = random_string_with_special_chars(8, 12).unwrap();
    let len = value.chars().count();
    assert!((8..=12).contains(&len));
    let first = value.chars().next().unwrap();
    assert!(TEST_DATA_CHARSET
        .chars()
        .any(|character| character == first));
    assert_eq!(
        test_data_charset_last_sorted(),
        TEST_DATA_CHARSET.chars().max().unwrap()
    );
}

#[test]
fn document_factories_emit_expected_shapes_and_partials() {
    assert_eq!(
        human_data(Some("p1"), Some(42), Some("Ada"))["passportId"],
        "p1"
    );
    assert_eq!(human_data(Some("p1"), Some(42), Some("Ada"))["age"], 42);
    assert!(simple_human_data()["lastName"].is_string());
    assert_eq!(simple_human_v3_data(Some(json!({ "age": 99 })))["age"], 99);
    assert_eq!(simple_human_age(None)["age"].as_str().unwrap().len(), 2);
    assert_eq!(
        nested_human_data(Some(json!({ "firstName": "Grace" })))["firstName"],
        "Grace"
    );
    assert_eq!(
        simple_hero_array(Some(json!({ "skills": ["jump"] })))["skills"],
        json!(["jump"])
    );
    assert_eq!(encrypted_human_data(Some("secret"))["secret"], "secret");
    assert_eq!(human_with_id_and_age_index_document_type(Some(7))["age"], 7);
    assert_eq!(
        human_with_ownership_data(None, "owner-a")["owner"],
        "owner-a"
    );
}

#[test]
fn nested_and_index_fixture_factories_cover_ported_surface() {
    assert!(human_with_sub_other()["other"]["age"].is_number());
    assert!(no_index_human()["firstName"].is_string());
    assert!(deep_nested_human_data()["mainSkill"]["attack"]["good"].is_boolean());
    assert!(big_human_document_type()["dnaHash"].is_string());
    assert_eq!(hero_array_data()["skills"].as_array().unwrap().len(), 3);
    assert!(encrypted_object_human_data()["secret"]["name"].is_string());
    assert!(encrypted_deep_human_document_type()["deepSecret"]["darkhole"]["pw"].is_string());
    assert!(compound_index_data()["passportCountry"].is_string());
    assert!(compound_index_no_string_data()["passportCountry"].is_object());
    assert!(nostring_index()["passportId"].is_object());
    assert_eq!(ref_human_data(Some("friend"))["bestFriend"], "friend");
    assert_eq!(
        ref_human_nested_data(Some("friend"))["foo"]["bestFriend"],
        "friend"
    );
    assert!(human_with_timestamp_data(None)["updatedAt"].is_number());
    assert_eq!(
        average_schema_data(None)["list"].as_array().unwrap().len(),
        5
    );
    assert!(point_data()["x"].is_number());
    assert_eq!(
        human_with_composite_primary(Some(json!({ "id": "Ada|Lovelace" })))["id"],
        "Ada|Lovelace"
    );
}
