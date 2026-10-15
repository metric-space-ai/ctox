// ref: sdk/cliproxy/auth/home_result.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: instance-owned usage manager replaces upstream package-global publication
// License: MIT (upstream); modifications AGPL-3.0-only

use std::time::SystemTime;

use crate::sdk::cliproxy::usage::{generate_flag, Failure, Record, UsageContext};

use super::{access_token_sha256, Auth, HomeAuthRuntime};

pub const HOME_RESULT_EXECUTOR_TYPE: &str = "home-result";

impl HomeAuthRuntime {
    /// Publishes a zero-token result for a Home-selected OAuth attempt whose
    /// 401 did not reach a provider usage reporter.
    pub fn report_home_unauthorized(
        &self,
        context: UsageContext,
        auth: &Auth,
        provider: &str,
        model: &str,
        observed_fingerprint: Option<&str>,
    ) -> bool {
        let manager = self
            .usage
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(manager) = manager else {
            return false;
        };
        let mut indexed = auth.clone();
        let auth_index = indexed.ensure_index();
        let fingerprint = observed_fingerprint
            .map(str::trim)
            .filter(|fingerprint| !fingerprint.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| access_token_sha256(auth));
        if auth_index.is_empty() || fingerprint.is_empty() {
            return false;
        }
        let model = model.trim().to_owned();
        let alias = if context.requested_model_alias().is_empty() {
            model.clone()
        } else {
            context.requested_model_alias().to_owned()
        };
        let reasoning_effort = context.reasoning_effort().to_owned();
        let service_tier = context.service_tier().to_owned();
        manager.publish(
            context,
            Record {
                provider: if provider.trim().is_empty() {
                    auth.provider.trim().to_owned()
                } else {
                    provider.trim().to_owned()
                },
                executor_type: HOME_RESULT_EXECUTOR_TYPE.to_owned(),
                model,
                alias,
                auth_id: auth.id.clone(),
                auth_index,
                access_token_sha256: fingerprint,
                auth_type: auth
                    .auth_kind()
                    .map(|kind| kind.as_str().to_owned())
                    .unwrap_or_default(),
                source: auth
                    .auth_source_kind()
                    .map(|source| source.as_str().to_owned())
                    .unwrap_or_default(),
                reasoning_effort,
                service_tier,
                requested_at: Some(SystemTime::now()),
                generate: generate_flag(false),
                failed: true,
                fail: Failure {
                    status_code: 401,
                    body: "upstream unauthorized".to_owned(),
                },
                ..Record::default()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::sdk::cliproxy::usage::{Manager, Plugin};

    use super::*;

    struct Collector(Arc<Mutex<Vec<Record>>>);

    impl Plugin for Collector {
        fn handle_usage(&self, _: &UsageContext, record: &Record) {
            self.0.lock().unwrap().push(record.clone());
        }
    }

}
