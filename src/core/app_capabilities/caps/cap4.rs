//! Native CAP-4 implementation; intentionally excluded from the serde-only contract crate.
use crate::app_capabilities::caps::NativeCall;
#[path = "cap4/task.rs"]
pub mod task;
#[path = "cap4/queue.rs"]
pub mod queue;
#[path = "cap4/schedule.rs"]
pub mod schedule;
#[path = "cap4/turn.rs"]
pub mod turn;
pub use task::*;
