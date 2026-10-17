//! Exact E55 claims shape; canonical byte equality is enforced by the parent.
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claims {
    instance_id: String,
    user_id: String,
    app_id: String,
    scope_hash: String,
    rights_ver: String,
    device_id: String,
    exp: u64,
    collections: Vec<Collection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Collection {
    collection: String,
    gen: u64,
    rights: Vec<String>,
    field_mask_digest: String,
}
pub(super) fn valid(value: &serde_json::Value) -> bool {
    let Ok(c) = serde_json::from_value::<Claims>(value.clone()) else {
        return false;
    };
    let bounded = |s: &str| !s.is_empty() && s.len() <= 256;
    bounded(&c.instance_id)
        && bounded(&c.user_id)
        && bounded(&c.app_id)
        && bounded(&c.device_id)
        && bounded(&c.scope_hash)
        && c.exp > 0
        && c.rights_ver
            .parse::<u64>()
            .is_ok_and(|n| n.to_string() == c.rights_ver)
        && !c.collections.is_empty()
        && c.collections.len() <= 256
        && c.collections
            .windows(2)
            .all(|w| w[0].collection < w[1].collection)
        && c.collections.iter().all(|x| {
            bounded(&x.collection)
                && x.gen > 0
                && bounded(&x.field_mask_digest)
                && x.rights.first().is_some_and(|r| r == "read")
                && (x.rights.len() == 1 || (x.rights.len() == 2 && x.rights[1] == "write"))
        })
}
