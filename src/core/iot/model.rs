// Origin: CTOX
// License: AGPL-3.0-only
//
// Asset / attribute / value / descriptor domain types plus the pure
// attribute-event semantics (§2A.1-6,8). Ported from OpenRemote.
//
// ref: Attribute.java:58-474
// ref: Asset.java:224-500
// ref: ValueDescriptor.java:62-301
// ref: AttributeEvent.java:148-220
// ref: AssetProcessingService.java:264-445

use crate::iot::Result;
use anyhow::{anyhow, bail};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// 2.1 Value type
// ---------------------------------------------------------------------------

/// Schema-flexible attribute value. JSON is the canonical wire/storage form;
/// numeric/boolean coercion happens at query/condition boundaries (§2A.9).
/// ref: Attribute.java:200-204,344-350 (lazy value hydration via valueStr)
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct AttributeValue(pub serde_json::Value);

impl AttributeValue {
    /// Coerce to f64 for numeric ops: Number → itself, Bool → 1.0/0.0,
    /// everything else → None (caller rejects; §2A.9).
    /// ref: AssetDatapointLTTBQuery.java:27-36
    pub(crate) fn as_numeric(&self) -> Option<f64> {
        match &self.0 {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub(crate) fn is_null(&self) -> bool {
        self.0.is_null()
    }
}

// ---------------------------------------------------------------------------
// 2.2 Value descriptor + registry
// ---------------------------------------------------------------------------

/// ref: ValueDescriptor.java:62-301
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ValueDescriptor {
    /// Globally unique; matches ^\w+(\[\])?$ . ref: ValueDescriptor.java:165-173
    pub name: String,
    pub base_type: ValueBaseType,
    #[serde(default)]
    pub array_dimensions: u32,
    #[serde(default)]
    pub constraints: Vec<serde_json::Value>,
    #[serde(default)]
    pub units: Option<Vec<String>>,
    #[serde(default)]
    pub format: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ValueBaseType {
    Number,
    Boolean,
    Text,
    Object,
    Array,
    GeoPoint,
}

impl ValueDescriptor {
    /// ref: ValueDescriptor.java:165-173 (asArray)
    pub(crate) fn as_array(&self) -> ValueDescriptor {
        let mut next = self.clone();
        // Strip a trailing `[]` so the dimension count is authoritative.
        let stripped = next.name.trim_end_matches("[]").to_string();
        next.name = format!("{stripped}[]");
        next.array_dimensions = self.array_dimensions.saturating_add(1).max(1);
        next
    }

    pub(crate) fn as_non_array(&self) -> ValueDescriptor {
        let mut next = self.clone();
        next.name = next.name.trim_end_matches("[]").to_string();
        next.array_dimensions = 0;
        next
    }
}

/// type-name → ordered attribute descriptors + per-attribute meta defaults.
/// ref: Asset.java:245-270 (AssetTypeInfo carried during deserialization)
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct AssetTypeInfo {
    pub asset_type: String,
    pub attributes: Vec<AttributeDescriptor>,
}

impl AssetTypeInfo {
    pub(crate) fn descriptor(&self, name: &str) -> Option<&AttributeDescriptor> {
        self.attributes.iter().find(|d| d.name == name)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AttributeDescriptor {
    pub name: String,
    pub value_descriptor: ValueDescriptor,
    /// merged into a new attribute ONCE at construction (§2A.6).
    #[serde(default)]
    pub meta: MetaMap,
}

// ---------------------------------------------------------------------------
// 2.3 Meta
// ---------------------------------------------------------------------------

/// ref: Attribute.java:300-306 (lazily-initialized MetaMap)
/// Null-or-empty meta are equivalence-equal in deep_equals (§2A.4).
pub(crate) type MetaMap = BTreeMap<String, serde_json::Value>;

// ---------------------------------------------------------------------------
// 2.4 Attribute
// ---------------------------------------------------------------------------

/// ref: Attribute.java:58-474
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Attribute {
    pub name: String,
    /// resolved value type; None until descriptor/type known (lazy hydration, §2A.8).
    #[serde(default)]
    pub value_type: Option<ValueBaseType>,
    #[serde(default)]
    pub value: Option<AttributeValue>,
    /// Unparsed JSON kept until type resolved. ref: Attribute.java:145 (valueStr)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub value_str: Option<String>,
    /// 0 == "no explicit timestamp" — distinct from epoch (§2A.1).
    /// ref: Attribute.java:204
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub meta: MetaMap,
}

impl Attribute {
    /// Bare attribute with no resolved type/value yet.
    pub(crate) fn new(name: impl Into<String>) -> Attribute {
        Attribute {
            name: name.into(),
            value_type: None,
            value: None,
            value_str: None,
            timestamp: 0,
            meta: MetaMap::new(),
        }
    }

    /// ref: Attribute.java:381-383  ( > 0, NOT >= 0 ) — §2A.1
    pub(crate) fn has_explicit_timestamp(&self) -> bool {
        self.timestamp > 0
    }

    /// Resolve value_str → value once the descriptor/type is known (§2A.8).
    /// ref: Attribute.java:344-350 (getValue parses on demand)
    pub(crate) fn hydrate(&mut self, ty: ValueBaseType) -> Result<()> {
        self.value_type = Some(ty);
        if self.value.is_none() {
            if let Some(raw) = self.value_str.take() {
                let parsed: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| anyhow!("failed to hydrate value_str for '{}': {e}", self.name))?;
                self.value = Some(AttributeValue(parsed));
            }
        }
        Ok(())
    }

    /// Shallow equality: (name, type, timestamp) only — change detection (§2A.4).
    /// ref: Attribute.java:425-450 (equals)
    pub(crate) fn shallow_eq(&self, other: &Attribute) -> bool {
        self.name == other.name
            && self.value_type == other.value_type
            && self.timestamp == other.timestamp
    }

    /// Deep equality: shallow + value (JSON fallback) + meta (null≈empty) (§2A.4).
    /// ref: Attribute.java:425-450 (deepEquals + objectsEqualsWithJSONFallback)
    pub(crate) fn deep_eq(&self, other: &Attribute) -> bool {
        if !self.shallow_eq(other) {
            return false;
        }
        if !values_equal_with_json_fallback(&self.value, &other.value) {
            return false;
        }
        meta_equivalent(&self.meta, &other.meta)
    }

    /// Merge descriptor meta into a freshly-constructed attribute ONCE; callers
    /// must never re-invoke on update (§2A.6). ref: Attribute.java:208-219
    pub(crate) fn merge_descriptor_meta_once(&mut self, descriptor: &AttributeDescriptor) {
        // Existing (explicit) meta wins over the descriptor default; the
        // descriptor only supplies keys the attribute does not already carry.
        for (key, value) in &descriptor.meta {
            self.meta
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        if self.value_type.is_none() {
            self.value_type = Some(descriptor.value_descriptor.base_type);
        }
    }
}

/// ref: Attribute.java:425-450 (objectsEqualsWithJSONFallback)
fn values_equal_with_json_fallback(a: &Option<AttributeValue>, b: &Option<AttributeValue>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.0 == y.0,
        _ => false,
    }
}

/// Null-or-empty meta are equivalence-equal (§2A.4). ref: Attribute.java:300-306
fn meta_equivalent(a: &MetaMap, b: &MetaMap) -> bool {
    a == b
}

// ---------------------------------------------------------------------------
// 2.5 Asset
// ---------------------------------------------------------------------------

/// ref: Asset.java:224-500
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Asset {
    /// 22-char Base62 UUID. ref: Asset.java (id field)
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub realm: String,
    /// discriminator / type name into AssetTypeInfo.
    pub asset_type: String,
    pub name: String,
    /// transient; computed from parent chain, may be empty on some loads.
    #[serde(default)]
    pub path: Vec<String>,
    /// keyed by attribute name. ref: Asset.java:329 (AttributeMap)
    #[serde(default)]
    pub attributes: BTreeMap<String, Attribute>,
}

impl Asset {
    /// New asset: for each descriptor in type_info, merge descriptor meta into
    /// the matching attribute ONCE (§2A.6). ref: Asset.java:341-342
    pub(crate) fn new_with_type(
        id: String,
        realm: String,
        asset_type: String,
        name: String,
        type_info: &AssetTypeInfo,
    ) -> Asset {
        let mut attributes = BTreeMap::new();
        for descriptor in &type_info.attributes {
            let mut attr = Attribute::new(descriptor.name.clone());
            attr.merge_descriptor_meta_once(descriptor);
            attributes.insert(attr.name.clone(), attr);
        }
        Asset {
            id,
            parent_id: None,
            realm,
            asset_type,
            name,
            path: Vec::new(),
            attributes,
        }
    }

    /// Generate a 22-char Base62 id. ref: UniqueIdentifierGenerator
    pub(crate) fn generate_id() -> String {
        let raw = uuid::Uuid::new_v4();
        base62_encode_u128(raw.as_u128())
    }
}

/// Encode a u128 as a fixed-width 22-char Base62 string (zero-padded).
/// ref: UniqueIdentifierGenerator (Base62, 22 chars)
fn base62_encode_u128(mut value: u128) -> String {
    const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut buf = [b'0'; 22];
    let mut idx = buf.len();
    if value == 0 {
        idx -= 1;
        buf[idx] = b'0';
    }
    while value > 0 && idx > 0 {
        idx -= 1;
        buf[idx] = ALPHABET[(value % 62) as usize];
        value /= 62;
    }
    String::from_utf8(buf.to_vec()).expect("base62 alphabet is ascii")
}

// ---------------------------------------------------------------------------
// 2.6 Attribute-event semantics (§2A.1-5 algorithm core — pure)
// ---------------------------------------------------------------------------

/// ref: AttributeEvent.java:148-220
#[derive(Clone, Debug)]
pub(crate) struct AttributeEvent {
    pub asset_id: String,
    pub attribute_name: String,
    pub value: AttributeValue,
    pub timestamp: i64,
    pub old_value: Option<AttributeValue>,
    pub old_value_timestamp: i64,
}

impl AttributeEvent {
    /// Outdated iff oldValueTimestamp > eventTimestamp (STRICTLY) — §2A.3.
    /// ref: AttributeEvent.java:219 (oldValueTimestamp - eventTimestamp > 0)
    pub(crate) fn is_outdated(&self) -> bool {
        self.old_value_timestamp - self.timestamp > 0
    }
}

/// Normalize an event timestamp against the system clock (§2A.2):
///   ts <= 0           → system_time_ms
///   ts > system_time  → clamp to system_time_ms (clock-drift guard)
/// ref: AssetProcessingService.java:279-285
pub(crate) fn normalize_event_timestamp(ts: i64, system_time_ms: i64) -> i64 {
    if ts <= 0 {
        system_time_ms
    } else if ts > system_time_ms {
        system_time_ms
    } else {
        ts
    }
}

/// Coerce an event value to the attribute's declared base type BEFORE
/// validation; coercion failure → Err (rejects whole event, §2A.5).
/// ref: AssetProcessingService.java:362-370 (ValueUtil.getValueCoerced)
pub(crate) fn coerce_value(
    value: &AttributeValue,
    target: ValueBaseType,
) -> Result<AttributeValue> {
    use serde_json::Value;
    // null passes through unchanged (a clear is always permitted).
    if value.is_null() {
        return Ok(value.clone());
    }
    let coerced = match target {
        ValueBaseType::Number => match &value.0 {
            Value::Number(_) => value.0.clone(),
            Value::Bool(b) => Value::from(if *b { 1.0 } else { 0.0 }),
            Value::String(s) => match s.parse::<f64>() {
                Ok(n) => serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .ok_or_else(|| anyhow!("non-finite number coercion"))?,
                Err(_) => bail!("cannot coerce '{s}' to Number"),
            },
            other => bail!("cannot coerce {other:?} to Number"),
        },
        ValueBaseType::Boolean => match &value.0 {
            Value::Bool(_) => value.0.clone(),
            Value::Number(n) => Value::Bool(n.as_f64().map(|f| f != 0.0).unwrap_or(false)),
            Value::String(s) => match s.as_str() {
                "true" | "TRUE" | "1" => Value::Bool(true),
                "false" | "FALSE" | "0" => Value::Bool(false),
                _ => bail!("cannot coerce '{s}' to Boolean"),
            },
            other => bail!("cannot coerce {other:?} to Boolean"),
        },
        ValueBaseType::Text => match &value.0 {
            Value::String(_) => value.0.clone(),
            Value::Number(n) => Value::String(n.to_string()),
            Value::Bool(b) => Value::String(b.to_string()),
            other => bail!("cannot coerce {other:?} to Text"),
        },
        ValueBaseType::Array => match &value.0 {
            Value::Array(_) => value.0.clone(),
            other => bail!("cannot coerce {other:?} to Array"),
        },
        ValueBaseType::Object | ValueBaseType::GeoPoint => match &value.0 {
            Value::Object(_) => value.0.clone(),
            other => bail!("cannot coerce {other:?} to Object"),
        },
    };
    Ok(AttributeValue(coerced))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn descriptor(name: &str, base: ValueBaseType, meta: MetaMap) -> AttributeDescriptor {
        AttributeDescriptor {
            name: name.to_string(),
            value_descriptor: ValueDescriptor {
                name: format!("{name}_vd"),
                base_type: base,
                array_dimensions: 0,
                constraints: vec![],
                units: None,
                format: None,
            },
            meta,
        }
    }

    // §2A.1 — has_explicit_timestamp: 0 vs >0 (strict > 0, NOT >= 0).
    #[test]
    fn has_explicit_timestamp_zero_vs_positive() {
        let mut a = Attribute::new("temp");
        assert!(!a.has_explicit_timestamp(), "0 means no explicit ts");
        a.timestamp = 1;
        assert!(a.has_explicit_timestamp());
        a.timestamp = 1_700_000_000_000;
        assert!(a.has_explicit_timestamp());
    }

    // §2A.2 — normalize_event_timestamp: <=0 → system time; future → clamp.
    #[test]
    fn normalize_timestamp_zero_and_future_clamp() {
        let sys = 1_000_000;
        assert_eq!(normalize_event_timestamp(0, sys), sys, "0 → system time");
        assert_eq!(normalize_event_timestamp(-5, sys), sys, "neg → system time");
        assert_eq!(
            normalize_event_timestamp(sys + 999, sys),
            sys,
            "future → clamp to system time"
        );
        assert_eq!(
            normalize_event_timestamp(sys - 10, sys),
            sys - 10,
            "valid past ts kept"
        );
        assert_eq!(normalize_event_timestamp(sys, sys), sys, "equal kept");
    }

    // §2A.3 — is_outdated is STRICT > (equal timestamps are not outdated).
    #[test]
    fn is_outdated_strict_greater() {
        let mut ev = AttributeEvent {
            asset_id: "a".into(),
            attribute_name: "temp".into(),
            value: AttributeValue(json!(1)),
            timestamp: 100,
            old_value: None,
            old_value_timestamp: 100,
        };
        assert!(!ev.is_outdated(), "equal ts is not outdated");
        ev.old_value_timestamp = 101;
        assert!(
            ev.is_outdated(),
            "older event than current state is outdated"
        );
        ev.old_value_timestamp = 99;
        assert!(!ev.is_outdated());
    }

    // §2A.4 — shallow vs deep equality.
    #[test]
    fn shallow_vs_deep_equality() {
        let mut a = Attribute::new("temp");
        a.value_type = Some(ValueBaseType::Number);
        a.timestamp = 10;
        a.value = Some(AttributeValue(json!(21.5)));

        let mut b = a.clone();
        b.value = Some(AttributeValue(json!(99.9))); // different value, same shallow

        assert!(a.shallow_eq(&b), "shallow ignores value");
        assert!(!a.deep_eq(&b), "deep compares value");

        let mut c = a.clone();
        assert!(a.deep_eq(&c), "identical is deep-equal");

        // null meta ≈ empty meta.
        c.meta.clear();
        assert!(a.deep_eq(&c));
        c.meta.insert("k".into(), json!(1));
        assert!(!a.deep_eq(&c), "differing meta breaks deep eq");
    }

    // §2A.5 — coercion failure rejects the event.
    #[test]
    fn coerce_value_failure_rejects() {
        assert!(coerce_value(&AttributeValue(json!("notnum")), ValueBaseType::Number).is_err());
        assert!(coerce_value(&AttributeValue(json!("maybe")), ValueBaseType::Boolean).is_err());
        // success paths
        assert_eq!(
            coerce_value(&AttributeValue(json!(true)), ValueBaseType::Number)
                .unwrap()
                .0,
            json!(1.0)
        );
        assert_eq!(
            coerce_value(&AttributeValue(json!("5")), ValueBaseType::Number)
                .unwrap()
                .as_numeric(),
            Some(5.0)
        );
        // null passes through.
        assert!(
            coerce_value(&AttributeValue(json!(null)), ValueBaseType::Number)
                .unwrap()
                .is_null()
        );
    }

    // §2A.6 — descriptor meta merged ONCE; explicit meta not overwritten.
    #[test]
    fn merge_descriptor_meta_once() {
        let mut meta = MetaMap::new();
        meta.insert("unit".into(), json!("celsius"));
        meta.insert("readOnly".into(), json!(true));
        let d = descriptor("temp", ValueBaseType::Number, meta);

        let mut attr = Attribute::new("temp");
        attr.meta.insert("unit".into(), json!("kelvin")); // explicit, must win
        attr.merge_descriptor_meta_once(&d);

        assert_eq!(
            attr.meta.get("unit"),
            Some(&json!("kelvin")),
            "explicit wins"
        );
        assert_eq!(
            attr.meta.get("readOnly"),
            Some(&json!(true)),
            "default filled"
        );
        assert_eq!(attr.value_type, Some(ValueBaseType::Number));

        // A second invocation must not change anything (idempotent / once).
        let before = attr.meta.clone();
        attr.merge_descriptor_meta_once(&d);
        assert_eq!(attr.meta, before, "re-merge is a no-op for present keys");
    }

    // §2A.6 — Asset::new_with_type merges descriptor meta into each attribute once.
    #[test]
    fn asset_new_with_type_merges_meta() {
        let mut m = MetaMap::new();
        m.insert("unit".into(), json!("celsius"));
        let info = AssetTypeInfo {
            asset_type: "Thermostat".into(),
            attributes: vec![descriptor("temp", ValueBaseType::Number, m)],
        };
        let asset = Asset::new_with_type(
            "id1".into(),
            "master".into(),
            "Thermostat".into(),
            "Living room".into(),
            &info,
        );
        let temp = asset.attributes.get("temp").expect("temp attr present");
        assert_eq!(temp.meta.get("unit"), Some(&json!("celsius")));
        assert_eq!(temp.value_type, Some(ValueBaseType::Number));
    }

    // §2A.8 — lazy hydration from value_str.
    #[test]
    fn hydrate_from_value_str() {
        let mut a = Attribute::new("temp");
        a.value_str = Some("42.5".to_string());
        assert!(a.value.is_none());
        a.hydrate(ValueBaseType::Number).unwrap();
        assert_eq!(a.value_type, Some(ValueBaseType::Number));
        assert_eq!(a.value.as_ref().unwrap().as_numeric(), Some(42.5));
        assert!(a.value_str.is_none(), "value_str consumed on hydrate");

        // Bad json → Err.
        let mut bad = Attribute::new("x");
        bad.value_str = Some("{not json".to_string());
        assert!(bad.hydrate(ValueBaseType::Object).is_err());
    }

    #[test]
    fn base62_id_is_22_chars() {
        let id = Asset::generate_id();
        assert_eq!(id.len(), 22, "id={id}");
        assert!(id.bytes().all(|b| b.is_ascii_alphanumeric()));
    }

    #[test]
    fn value_descriptor_array_roundtrip() {
        let vd = ValueDescriptor {
            name: "number".into(),
            base_type: ValueBaseType::Number,
            array_dimensions: 0,
            constraints: vec![],
            units: None,
            format: None,
        };
        let arr = vd.as_array();
        assert_eq!(arr.name, "number[]");
        assert_eq!(arr.array_dimensions, 1);
        let back = arr.as_non_array();
        assert_eq!(back.name, "number");
        assert_eq!(back.array_dimensions, 0);
    }
}
