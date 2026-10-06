// ref: sdk/auth/meta.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned UI presentation and existing injected Manager store
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    Authenticator, AuthenticatorError, AuthenticatorErrorKind, LoginCancellation, LoginConfig,
    LoginFuture, LoginOptions, PromptError,
};
use crate::internal::auth::meta::{credential_file_name, MetaAuth, MetaAuthError};
use crate::sdk::cliproxy::auth::{Auth, AuthStatus};
use serde_json::Value;
use std::{fmt, sync::Arc};

pub struct MetaDevicePresentation {
    pub verification_url: String,
    pub user_code: String,
    pub expires_in_seconds: i64,
    pub automatic_browser_allowed: bool,
}
impl fmt::Debug for MetaDevicePresentation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaDevicePresentation")
            .field("expires_in_seconds", &self.expires_in_seconds)
            .field("automatic_browser_allowed", &self.automatic_browser_allowed)
            .finish_non_exhaustive()
    }
}
/// Host presentation must return promptly; the SDK never owns a browser launcher.
pub trait MetaLoginPresenter: Send + Sync {
    fn present(&self, challenge: &MetaDevicePresentation) -> Result<(), PromptError>;
}
pub struct MetaAuthenticator {
    service: Arc<MetaAuth>,
    presenter: Arc<dyn MetaLoginPresenter>,
}
impl MetaAuthenticator {
    pub fn new(service: Arc<MetaAuth>, presenter: Arc<dyn MetaLoginPresenter>) -> Self {
        Self { service, presenter }
    }
}
impl fmt::Debug for MetaAuthenticator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaAuthenticator").finish_non_exhaustive()
    }
}
impl Authenticator for MetaAuthenticator {
    fn provider(&self) -> &str {
        "meta"
    }
    // No scheduled refresh lead: API keys do not advertise an expiration.
    fn login<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        _config: &'a LoginConfig,
        options: &'a LoginOptions,
    ) -> LoginFuture<'a> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(cancelled());
            }
            let operation = async {
                let device = self.service.start_device_flow().await.map_err(auth_error)?;
                let verification_url = if device.verification_uri_complete.trim().is_empty() {
                    device.verification_uri.trim()
                } else {
                    device.verification_uri_complete.trim()
                };
                if verification_url.is_empty() {
                    return Err(AuthenticatorError::new(
                        AuthenticatorErrorKind::InvalidRecord,
                    ));
                }
                if cancellation.is_cancelled() {
                    return Err(cancelled());
                }
                self.presenter
                    .present(&MetaDevicePresentation {
                        verification_url: verification_url.into(),
                        user_code: device.user_code.clone(),
                        expires_in_seconds: device.expires_in,
                        automatic_browser_allowed: !options.no_browser,
                    })
                    .map_err(|error| {
                        AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
                    })?;
                let bundle = self
                    .service
                    .wait_for_authorization(&device)
                    .await
                    .map_err(auth_error)?;
                let storage = self.service.create_token_storage(&bundle).ok_or_else(|| {
                    AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord)
                })?;
                if storage.access_token.trim().is_empty() {
                    return Err(AuthenticatorError::new(
                        AuthenticatorErrorKind::InvalidRecord,
                    ));
                }
                let id = credential_file_name(&storage.email, &storage.dca_token);
                let mut record = Auth::default();
                record.id = id.clone();
                record.file_name = id;
                record.provider = "meta".into();
                record.status = AuthStatus::Active;
                record.label = if storage.email.trim().is_empty() {
                    "Meta".into()
                } else {
                    storage.email.trim().into()
                };
                record.metadata = storage.snapshot(None);
                // Upstream's SDK keeps these keys even when a lifecycle value is cleared.
                record.metadata.insert(
                    "token_type".into(),
                    Value::String(storage.token_type.clone()),
                );
                record
                    .metadata
                    .insert("expires_in".into(), Value::from(storage.expires_in));
                record
                    .metadata
                    .insert("expired".into(), Value::String(storage.expired.clone()));
                if let Some(minted) = &bundle.minted_key {
                    for (key, value) in [
                        ("subs_tier_name", &minted.subs_tier_name),
                        ("subs_tier_id", &minted.subs_tier_id),
                    ] {
                        record
                            .metadata
                            .insert(key.into(), Value::String(value.clone()));
                    }
                    record
                        .metadata
                        .insert("is_subs_active".into(), Value::Bool(minted.is_subs_active));
                    record.metadata.insert(
                        "has_payment_method".into(),
                        Value::Bool(minted.has_payment_method),
                    );
                }
                record.attributes.insert("auth_kind".into(), "oauth".into());
                record
                    .attributes
                    .insert("base_url".into(), storage.base_url.clone());
                for (key, value) in [
                    ("api_key", &storage.api_key),
                    ("dca_token", &storage.dca_token),
                    ("email", &storage.email),
                ] {
                    if !value.is_empty() {
                        record.attributes.insert(key.into(), value.clone());
                    }
                }
                if cancellation.is_cancelled() {
                    return Err(cancelled());
                }
                Ok(Some(record))
            };
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(cancelled()),
                result = operation => result,
            }
        })
    }
}
fn cancelled() -> AuthenticatorError {
    AuthenticatorError::new(AuthenticatorErrorKind::Cancelled)
}
fn auth_error(error: MetaAuthError) -> AuthenticatorError {
    let kind = match error {
        MetaAuthError::MissingDeviceResponse
        | MetaAuthError::MissingDcaToken
        | MetaAuthError::MissingField(_) => AuthenticatorErrorKind::InvalidRecord,
        _ => AuthenticatorErrorKind::LoginFailed,
    };
    AuthenticatorError::with_source(kind, error)
}
#[cfg(test)]
#[path = "meta_test.rs"]
mod tests;
