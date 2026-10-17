use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;

pub(super) const ROUTES: &[NativeRoute] = &[NativeRoute::new(
    "knowledge",
    "query",
    "knowledge.command",
    "knowledge::dispatch_capturing",
    "src/core/knowledge/mod.rs:109",
    PayloadShape::Knowledge,
)];
