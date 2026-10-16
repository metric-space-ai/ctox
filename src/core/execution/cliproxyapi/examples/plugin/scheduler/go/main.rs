// ref: examples/plugin/scheduler/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleRegistration};
pub fn example() -> ExampleRegistration {
    registration("example-scheduler", &["scheduler"])
}
#[derive(Clone, Debug, Default)]
pub struct SchedulerConfig {
    pub auth_id: String,
    pub delegate: String,
    pub deny: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Unhandled,
    Auth(String),
    Delegate(String),
    Denied,
}
pub fn pick(cfg: &SchedulerConfig, candidates: &[&str]) -> Pick {
    if cfg.deny {
        Pick::Denied
    } else if matches!(cfg.delegate.as_str(), "fill-first" | "round-robin") {
        Pick::Delegate(cfg.delegate.clone())
    } else if candidates.contains(&cfg.auth_id.as_str()) {
        Pick::Auth(cfg.auth_id.clone())
    } else {
        Pick::Unhandled
    }
}
