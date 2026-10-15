//! Fixed typed native destinations. A runtime cannot choose an arbitrary command.
use super::schema::{FunctionSchema, PayloadShape};
use super::CtoxError;
use serde_json::Value;
mod coding;
pub mod queue;
pub mod schedule;
pub mod task;
pub mod turn;

#[derive(Debug, Clone, Copy)]
pub struct NativeRoute {
    pub capability: &'static str,
    pub function: &'static str,
    command_type: &'static str,
    pub rust_function: &'static str,
    pub source: &'static str,
    pub schema: FunctionSchema,
}
impl NativeRoute {
    pub(crate) const fn new(
        capability: &'static str,
        function: &'static str,
        command_type: &'static str,
        rust_function: &'static str,
        source: &'static str,
        payload: PayloadShape,
    ) -> Self {
        Self {
            capability,
            function,
            command_type,
            rust_function,
            source,
            schema: FunctionSchema {
                payload,
                action: None,
            },
        }
    }
    pub fn command_type(&self) -> &'static str {
        self.command_type
    }
    pub(crate) fn call(&self, args: &Value) -> Result<NativeCall, CtoxError> {
        self.schema.validate(args)?;
        Ok(NativeCall {
            route: *self,
            payload: args["payload"].clone(),
            record_id: args
                .get("record_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
}
#[derive(Debug, Clone)]
pub struct NativeCall {
    route: NativeRoute,
    payload: Value,
    record_id: Option<String>,
}
impl NativeCall {
    pub fn route(&self) -> &NativeRoute {
        &self.route
    }
    pub fn payload(&self) -> &Value {
        &self.payload
    }
    pub fn record_id(&self) -> Option<&str> {
        self.record_id.as_deref()
    }
}
pub fn all_routes() -> impl Iterator<Item = &'static NativeRoute> {
    [
        queue::ROUTES,
        task::ROUTES,
        turn::ROUTES,
        coding::ROUTES,
        schedule::ROUTES,
    ]
    .into_iter()
    .flatten()
}
