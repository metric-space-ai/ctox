// ref: internal/runtime/executor/meta_executor.go:91-235,249-333 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned selected transport and manager-only publication
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::auth::meta::{
    MetaAuth, MetaAuthError, MetaClock, SystemMetaClock, DEFAULT_API_BASE_URL,
};
use crate::sdk::cliproxy::auth::{
    is_config_api_key_auth, AsyncAuthRefresher, Auth, AuthError, AuthPreparationError, AuthPreparer,
};
use chrono::SecondsFormat;
use futures_util::future::{BoxFuture, FutureExt, Shared};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
};
use zeroize::Zeroizing;

/// The native Meta SDK places its owned token snapshot in Auth.metadata.
/// The shared storage writer is never read, mutated or persisted by this executor.
pub struct MetaCredentials {
    base_url: String,
    api_key: Zeroizing<String>,
}
impl MetaCredentials {
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn api_key(&self) -> &str {
        self.api_key.as_str()
    }
}
impl fmt::Debug for MetaCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaCredentials")
            .field("has_api_key", &!self.api_key.is_empty())
            .finish_non_exhaustive()
    }
}

/// Match upstream precedence, including the default-URL metadata fallback.
/// A DCA token is never an inference API key, regardless of its field name.
pub fn meta_credentials(auth: &Auth) -> MetaCredentials {
    let mut base_url = attribute(auth, "base_url")
        .unwrap_or(DEFAULT_API_BASE_URL)
        .to_owned();
    if base_url == DEFAULT_API_BASE_URL {
        if let Some(value) = metadata(auth, "base_url").or_else(|| metadata(auth, "api_base_url")) {
            base_url = value.to_owned();
        }
    }
    let api_key = attribute(auth, "api_key")
        .filter(|value| !value.starts_with("dca:"))
        .or_else(|| attribute(auth, "access_token").filter(|value| !value.starts_with("dca:")))
        .or_else(|| metadata(auth, "api_key").filter(|value| !value.starts_with("dca:")))
        .or_else(|| metadata(auth, "access_token").filter(|value| !value.starts_with("dca:")))
        .unwrap_or_default();
    MetaCredentials {
        base_url,
        api_key: Zeroizing::new(api_key.to_owned()),
    }
}

pub fn meta_dca_token(auth: &Auth) -> Zeroizing<String> {
    if is_config_api_key_auth(Some(auth)) {
        return Zeroizing::new(String::new());
    }
    let token = attribute(auth, "dca_token")
        .or_else(|| attribute(auth, "access_token").filter(|value| value.starts_with("dca:")))
        .or_else(|| metadata(auth, "dca_token"))
        .or_else(|| metadata(auth, "access_token").filter(|value| value.starts_with("dca:")))
        .unwrap_or_default();
    Zeroizing::new(token.to_owned())
}
fn attribute<'a>(auth: &'a Auth, key: &str) -> Option<&'a str> {
    auth.attributes
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}
fn metadata<'a>(auth: &'a Auth, key: &str) -> Option<&'a str> {
    auth.metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

type MintResult = Result<Arc<crate::internal::auth::meta::MintedKeyResponse>, Arc<MetaAuthError>>;
struct MintFlight {
    future: Shared<BoxFuture<'static, MintResult>>,
}

/// Owner-injected equivalent of upstream's DCA-token singleflight group.
/// Weak entries do not keep abandoned HTTP operations alive. The first completed
/// waiter removes its flight, so a later 401 performs a fresh mint.
#[derive(Default)]
pub struct MetaMintCoordinator {
    flights: Mutex<BTreeMap<[u8; 32], Weak<MintFlight>>>,
}
impl MetaMintCoordinator {
    async fn mint(&self, service: Arc<MetaAuth>, token: Zeroizing<String>) -> MintResult {
        let key: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let flight = {
            let mut flights = self
                .flights
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            flights.retain(|_, entry| entry.strong_count() > 0);
            if let Some(flight) = flights.get(&key).and_then(Weak::upgrade) {
                flight
            } else {
                let future = async move {
                    service
                        .mint_api_key(token.as_str())
                        .await
                        .map(Arc::new)
                        .map_err(Arc::new)
                }
                .boxed()
                .shared();
                let flight = Arc::new(MintFlight { future });
                flights.insert(key, Arc::downgrade(&flight));
                flight
            }
        };
        let result = flight.future.clone().await;
        let mut flights = self
            .flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if flights
            .get(&key)
            .and_then(Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, &flight))
        {
            flights.remove(&key);
        }
        result
    }
}
impl fmt::Debug for MetaMintCoordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaMintCoordinator")
            .finish_non_exhaustive()
    }
}

/// One capability is supplied to both preparation and asynchronous 401 refresh.
/// GenericAuthRuntime serializes them per account and validates the base epoch
/// before saving and returning a candidate to inference.
pub struct MetaRequestAuthPreparer {
    service: Arc<MetaAuth>,
    clock: Arc<dyn MetaClock>,
    mints: Arc<MetaMintCoordinator>,
}
impl MetaRequestAuthPreparer {
    pub fn new(service: Arc<MetaAuth>) -> Self {
        Self {
            service,
            clock: Arc::new(SystemMetaClock),
            mints: Arc::new(MetaMintCoordinator::default()),
        }
    }
    pub fn with_mint_coordinator(mut self, coordinator: Arc<MetaMintCoordinator>) -> Self {
        self.mints = coordinator;
        self
    }
    pub fn with_clock(mut self, clock: Arc<dyn MetaClock>) -> Self {
        self.clock = clock;
        self
    }
    pub async fn refresh_candidate(&self, auth: &Auth) -> Result<Auth, AuthPreparationError> {
        if !auth.provider.trim().eq_ignore_ascii_case("meta") {
            return Err(credential_error(
                "Meta refresh requires the selected Meta account",
            ));
        }
        let dca_token = meta_dca_token(auth);
        if dca_token.is_empty() {
            if !meta_credentials(auth).api_key().is_empty() {
                return Ok(auth.clone());
            }
            return Err(credential_error(
                "Meta account requires an inference API key or an OAuth DCA token",
            ));
        }
        // The selected service owns proxy/TLS, the fixed mint endpoint, bounded
        // body and thirty-second deadline. Dropping this await cancels its receiver.
        let minted = self
            .mints
            .mint(self.service.clone(), dca_token.clone())
            .await
            .map_err(|error| mint_error(error.as_ref()))?;
        let mut candidate = auth.clone();
        let base_url = if minted.base_url.trim().is_empty() {
            meta_credentials(auth).base_url().to_owned()
        } else {
            minted.base_url.trim().to_owned()
        };
        let now = self.clock.now();
        for (key, value) in [
            ("base_url", base_url.as_str()),
            ("api_key", minted.api_key.as_str()),
            ("access_token", minted.api_key.as_str()),
            ("dca_token", dca_token.as_str()),
            ("type", "meta"),
        ] {
            candidate
                .metadata
                .insert(key.into(), Value::String(value.into()));
        }
        candidate.metadata.remove("expired");
        // DCA expiry stays independent; minted API keys do not advertise expiry.
        for (key, value) in [
            ("email", minted.user_email.as_str()),
            ("name", minted.user_full_name.as_str()),
        ] {
            if !value.is_empty() {
                candidate
                    .metadata
                    .insert(key.into(), Value::String(value.into()));
            }
        }
        for (key, value) in [
            ("subs_tier_name", minted.subs_tier_name.as_str()),
            ("subs_tier_id", minted.subs_tier_id.as_str()),
        ] {
            if value.is_empty() {
                candidate.metadata.remove(key);
            } else {
                candidate
                    .metadata
                    .insert(key.into(), Value::String(value.into()));
            }
        }
        candidate
            .metadata
            .insert("is_subs_active".into(), Value::Bool(minted.is_subs_active));
        candidate.metadata.insert(
            "has_payment_method".into(),
            Value::Bool(minted.has_payment_method),
        );
        candidate.metadata.insert(
            "last_refresh".into(),
            Value::String(now.to_rfc3339_opts(SecondsFormat::Secs, true)),
        );
        for (key, value) in [
            ("base_url", base_url),
            ("api_key", minted.api_key.clone()),
            ("access_token", minted.api_key.clone()),
        ] {
            candidate.attributes.insert(key.into(), value);
        }
        candidate.last_refreshed_at = now;
        Ok(candidate)
    }
}
impl fmt::Debug for MetaRequestAuthPreparer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaRequestAuthPreparer")
            .finish_non_exhaustive()
    }
}
impl AuthPreparer for MetaRequestAuthPreparer {
    fn should_prepare(&self, auth: &Auth) -> bool {
        auth.provider.trim().eq_ignore_ascii_case("meta")
            && !is_config_api_key_auth(Some(auth))
            && meta_credentials(auth).api_key().is_empty()
            && !meta_dca_token(auth).is_empty()
    }
    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthPreparationError>> + Send + 'a>> {
        Box::pin(async move {
            if self.should_prepare(auth) {
                // Write only after mint succeeds; the manager still owns acceptance.
                *auth = self.refresh_candidate(auth).await?;
            }
            Ok(())
        })
    }
}
impl AsyncAuthRefresher for MetaRequestAuthPreparer {
    fn should_refresh(&self, auth: &Auth) -> bool {
        auth.provider.trim().eq_ignore_ascii_case("meta")
            && !is_config_api_key_auth(Some(auth))
            && (auth.auth_kind() == Some(crate::sdk::cliproxy::auth::AuthKind::OAuth)
                || !meta_dca_token(auth).is_empty())
    }
    fn credential_changed(&self, current: &Auth, failed: &Auth) -> bool {
        meta_credentials(current).api_key() != meta_credentials(failed).api_key()
    }
    fn refresh<'a>(
        &'a self,
        auth: &'a Auth,
    ) -> Pin<Box<dyn Future<Output = Result<Auth, AuthPreparationError>> + Send + 'a>> {
        Box::pin(self.refresh_candidate(auth))
    }
}
fn credential_error(message: &'static str) -> AuthPreparationError {
    Arc::new(AuthError {
        code: "meta_credentials".into(),
        message: message.into(),
        retryable: false,
        http_status: 401,
    })
}
fn mint_error(error: &MetaAuthError) -> AuthPreparationError {
    let status = match error {
        MetaAuthError::Upstream { status, .. } => *status,
        MetaAuthError::Timeout => 408,
        MetaAuthError::MissingDcaToken | MetaAuthError::AccessDenied => 401,
        _ => 502,
    };
    Arc::new(AuthError {
        code: "meta_mint".into(),
        message: format!("Meta API key mint failed: {error}"),
        retryable: status == 408 || status == 429 || status >= 500,
        http_status: status,
    })
}
#[cfg(test)]
#[path = "meta_executor_auth_test.rs"]
mod tests;
