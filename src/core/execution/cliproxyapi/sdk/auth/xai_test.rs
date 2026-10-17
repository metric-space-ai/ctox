// ref: sdk/auth/xai_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::{Authenticator, XaiAuthenticator, XaiHandleFactory, XaiLoginPresenter};
use crate::internal::auth::xai::{
    SystemXaiClock, XaiAuth, XaiClock, XaiCredentialHandles, XaiHttpFuture, XaiHttpRequest,
    XaiHttpResponse, XaiHttpTransport, XaiRefreshCoordinator, XaiSecretHandle, XaiSecretKind,
    XaiSecretStore, XaiSecretStoreError, XaiSleepFuture, XaiStoredCredentials, XaiTransportFailure,
    REFRESH_LEAD,
};
use crate::sdk::auth::{LoginCancellation, LoginConfig, LoginOptions, PromptError};

struct NoTransport;
impl XaiHttpTransport for NoTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a XaiHttpRequest,
        _timeout: Duration,
        _cancellation: &'a LoginCancellation,
    ) -> XaiHttpFuture<'a> {
        Box::pin(async { Err(XaiTransportFailure::Protocol) })
    }
}
struct NoStore;
impl XaiSecretStore for NoStore {
    fn load_credentials(
        &self,
        _handles: &XaiCredentialHandles,
    ) -> Result<XaiStoredCredentials, XaiSecretStoreError> {
        Err(XaiSecretStoreError::Missing)
    }
    fn store_credentials(
        &self,
        _handles: &XaiCredentialHandles,
        _credentials: &XaiStoredCredentials,
    ) -> Result<(), XaiSecretStoreError> {
        Ok(())
    }
}
struct NoHandles;
impl XaiHandleFactory for NoHandles {
    fn handles_for(&self, _record_id: &str) -> Result<XaiCredentialHandles, XaiSecretStoreError> {
        Err(XaiSecretStoreError::Missing)
    }
}
struct NoPresenter;
impl XaiLoginPresenter for NoPresenter {
    fn present(&self, _challenge: &super::XaiDevicePresentation) -> Result<(), PromptError> {
        Ok(())
    }
}

struct FixedClock(Mutex<SystemTime>);
impl XaiClock for FixedClock {
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap()
    }
    fn sleep<'a>(
        &'a self,
        duration: Duration,
        _cancellation: &'a LoginCancellation,
    ) -> XaiSleepFuture<'a> {
        Box::pin(async move {
            *self.0.lock().unwrap() += duration;
            Ok(())
        })
    }
}

struct SequenceTransport(Mutex<VecDeque<XaiHttpResponse>>);
impl XaiHttpTransport for SequenceTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a XaiHttpRequest,
        _timeout: Duration,
        _cancellation: &'a LoginCancellation,
    ) -> XaiHttpFuture<'a> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(XaiTransportFailure::Protocol)
        })
    }
}

#[derive(Default)]
struct RecordingStore(Mutex<Option<XaiStoredCredentials>>);
impl XaiSecretStore for RecordingStore {
    fn load_credentials(
        &self,
        _handles: &XaiCredentialHandles,
    ) -> Result<XaiStoredCredentials, XaiSecretStoreError> {
        self.0
            .lock()
            .unwrap()
            .clone()
            .ok_or(XaiSecretStoreError::Missing)
    }
    fn store_credentials(
        &self,
        _handles: &XaiCredentialHandles,
        credentials: &XaiStoredCredentials,
    ) -> Result<(), XaiSecretStoreError> {
        *self.0.lock().unwrap() = Some(credentials.clone());
        Ok(())
    }
}

struct Handles;
impl XaiHandleFactory for Handles {
    fn handles_for(&self, record_id: &str) -> Result<XaiCredentialHandles, XaiSecretStoreError> {
        Ok(XaiCredentialHandles {
            access: XaiSecretHandle::new(format!("{record_id}/access"), XaiSecretKind::Access)
                .unwrap(),
            refresh: XaiSecretHandle::new(format!("{record_id}/refresh"), XaiSecretKind::Refresh)
                .unwrap(),
            identity: XaiSecretHandle::new(
                format!("{record_id}/identity"),
                XaiSecretKind::Identity,
            )
            .unwrap(),
        })
    }
}

#[derive(Default)]
struct Presenter(Mutex<Vec<super::XaiDevicePresentation>>);
impl XaiLoginPresenter for Presenter {
    fn present(&self, challenge: &super::XaiDevicePresentation) -> Result<(), PromptError> {
        self.0.lock().unwrap().push(challenge.clone());
        Ok(())
    }
}

#[tokio::test]
async fn login_keeps_secrets_out_of_metadata_and_persists_only_via_injected_store() {
    let clock = Arc::new(FixedClock(Mutex::new(SystemTime::UNIX_EPOCH)));
    let transport = Arc::new(SequenceTransport(Mutex::new(VecDeque::from([
        XaiHttpResponse::new(200, br#"{"device_authorization_endpoint":"https://auth.x.ai/device","token_endpoint":"https://auth.x.ai/token"}"#.to_vec()),
        XaiHttpResponse::new(200, br#"{"device_code":"device","user_code":"USER","verification_uri":"https://auth.x.ai/verify","expires_in":60,"interval":5}"#.to_vec()),
        XaiHttpResponse::new(200, br#"{"access_token":"access-secret","refresh_token":"refresh-secret","id_token":"identity-secret","token_type":"Bearer","expires_in":3600}"#.to_vec()),
    ]))));
    let service = Arc::new(XaiAuth::new(
        transport,
        clock.clone(),
        Arc::new(XaiRefreshCoordinator::default()),
    ));
    let store = Arc::new(RecordingStore::default());
    let presenter = Arc::new(Presenter::default());
    let authenticator = XaiAuthenticator::new(
        service,
        clock,
        store.clone(),
        Arc::new(Handles),
        presenter.clone(),
    );
    let mut record = authenticator
        .login(
            &LoginCancellation::default(),
            &LoginConfig::default(),
            &LoginOptions::default(),
        )
        .await
        .unwrap()
        .unwrap();
    let encoded = serde_json::to_string(&record.metadata).unwrap();
    for secret in ["access-secret", "refresh-secret", "identity-secret"] {
        assert!(!encoded.contains(secret));
    }
    assert_eq!(presenter.0.lock().unwrap()[0].user_code, "USER");
    record
        .storage
        .take()
        .unwrap()
        .lock()
        .unwrap()
        .save_token_to_file(Path::new("ignored-by-injected-store"))
        .unwrap();
    let persisted = store.0.lock().unwrap().clone().unwrap();
    assert_eq!(persisted.access_token().expose_secret(), "access-secret");
    assert_eq!(
        persisted.refresh_token().unwrap().expose_secret(),
        "refresh-secret"
    );
    assert_eq!(
        persisted.id_token().unwrap().expose_secret(),
        "identity-secret"
    );
}
