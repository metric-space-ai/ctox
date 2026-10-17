// Origin: CTOX native Meta reset/account-scope conductor guards
// License: AGPL-3.0-only
use super::*;
use crate::internal::runtime::executor::meta_executor_response::{
    meta_upstream_error, MetaHttpStatusError,
};
use std::time::{Duration, SystemTime};
const NOW: SystemTime = SystemTime::UNIX_EPOCH;
fn account(provider: &str) -> Auth {
    let mut auth = Auth::default();
    auth.id = "owned-test-account".into();
    auth.provider = provider.into();
    auth
}
fn error(status: u16, body: &[u8]) -> PluginExecutionError {
    Arc::new(meta_upstream_error(status, body, NOW))
}

#[derive(Debug)]
struct Wrapped(MetaHttpStatusError);
impl fmt::Display for Wrapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("owned failure wrapper")
    }
}
impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
