//! Small closed schema vocabulary, not a permissive partial JSON-schema engine.
//! The same typed definition validates and emits the public JSON schema.
use super::CtoxError;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy)]
pub enum PayloadShape {
    Object,
    Knowledge,
}

#[derive(Debug, Clone, Copy)]
pub struct FunctionSchema {
    pub payload: PayloadShape,
    pub action: Option<&'static [&'static str]>,
}
impl FunctionSchema {
    pub fn validate(&self, args: &Value) -> Result<(), CtoxError> {
        let object = args
            .as_object()
            .ok_or_else(|| CtoxError::invalid("$.args"))?;
        if object
            .keys()
            .any(|key| !["payload", "record_id", "action"].contains(&key.as_str()))
        {
            return Err(CtoxError::invalid("$.args"));
        }
        if let Some(actions) = self.action {
            let action = object
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| CtoxError::invalid("$.args.action"))?;
            if !actions.contains(&action) {
                return Err(CtoxError::invalid("$.args.action"));
            }
        } else if object.contains_key("action") {
            return Err(CtoxError::invalid("$.args.action"));
        }
        let payload = object
            .get("payload")
            .and_then(Value::as_object)
            .ok_or_else(|| CtoxError::invalid("$.args.payload"))?;
        if let Some(record_id) = object.get("record_id") {
            super::protocol::identifier(
                record_id
                    .as_str()
                    .ok_or_else(|| CtoxError::invalid("$.args.record_id"))?,
                "$.args.record_id",
            )?;
        }
        if matches!(self.payload, PayloadShape::Knowledge) {
            let items = payload
                .get("args")
                .and_then(Value::as_array)
                .ok_or_else(|| CtoxError::invalid("$.args.payload.args"))?;
            if items.len() > 256 || items.iter().any(|v| v.as_str().is_none()) {
                return Err(CtoxError::invalid("$.args.payload.args"));
            }
            if payload.len() != 1 {
                return Err(CtoxError::invalid("$.args.payload"));
            }
        }
        Ok(())
    }
    pub fn json(&self) -> Value {
        let payload = match self.payload {
            PayloadShape::Object => json!({"type":"object"}),
            PayloadShape::Knowledge => {
                json!({"type":"object","additionalProperties":false,"required":["args"],
                "properties":{"args":{"type":"array","maxItems":256,"items":{"type":"string"}}}})
            }
        };
        let mut schema = json!({"type":"object","additionalProperties":false,"required":["payload"],
            "properties":{"payload":payload,"record_id":{"type":"string","minLength":1,"maxLength":256}}});
        if let Some(actions) = self.action {
            schema["properties"]["action"] = json!({"type":"string","enum":actions});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("action"));
        }
        schema
    }
}
