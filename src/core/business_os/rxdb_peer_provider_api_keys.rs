// Origin: CTOX
// License: AGPL-3.0-only
//! Transient, authenticated Owner/Admin credential ingress. No key is serialized
//! into a business command, registry projection, response or diagnostic.
use super::{provider_federation, store};
use crate::execution::cliproxyapi_integration::{
    install_api_key_account_owned_authorized, ApiKeyProvider,
};
use anyhow::{ensure, Context};
use rxdb::plugins::replication_webrtc::{
    index_mod::GuardedAuxiliaryResponse, WebRTCPublicationGuard,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};
use zeroize::Zeroizing;
pub(super) const METHOD: &str = "ctox.workjet.providers.api-key.v1";
pub(super) const CAPABILITY: &str = "ctox-workjet-provider-api-key-v1";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    version: u8,
    action: String,
    command_id: String,
    provider: ApiKeyProvider,
    secret: String,
}
struct Authority {
    root: PathBuf,
    token: String,
    owner: String,
    actor: String,
    actor_epoch: i64,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Authority {
    fn capture(
        root: PathBuf,
        token: String,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> anyhow::Result<Self> {
        ensure!(current(), "provider credential connection retired");
        let claims = store::verified_webrtc_capability_claims(&root, &token)
            .context("provider credential authority unavailable")?;
        ensure!(
            matches!(claims.role.as_str(), "chef" | "admin"),
            "provider credential ingress requires Owner/Admin"
        );
        let conn = store::open_store(&root)?;
        let owner = provider_federation::management_owner(&conn, &claims.user_id)?;
        Ok(Self {
            root,
            token,
            owner,
            actor: claims.user_id,
            actor_epoch: claims.actor_epoch,
            current,
        })
    }
    fn check(&self) -> anyhow::Result<()> {
        ensure!((self.current)(), "provider credential connection retired");
        let claims = store::verified_webrtc_capability_claims(&self.root, &self.token)
            .context("provider credential authority unavailable")?;
        ensure!(
            claims.user_id == self.actor
                && claims.actor_epoch == self.actor_epoch
                && matches!(claims.role.as_str(), "chef" | "admin"),
            "provider credential authority changed"
        );
        let conn = store::open_store(&self.root)?;
        ensure!(
            provider_federation::management_owner(&conn, &claims.user_id)? == self.owner
                && (self.current)(),
            "provider credential owner changed"
        );
        Ok(())
    }
}
impl WebRTCPublicationGuard for Authority {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        self.check()
            .map_err(|_| rxdb::rx_error::new_rx_error("PROVIDER_API_KEY_RETIRED", None))?;
        publish()
    }
}
async fn handle(
    authority: Arc<Authority>,
    params: Vec<Value>,
) -> anyhow::Result<GuardedAuxiliaryResponse> {
    ensure!(
        params.len() == 1 && serde_json::to_vec(&params)?.len() <= 16384,
        "invalid provider credential request"
    );
    let request: Request = serde_json::from_value(params.into_iter().next().unwrap())?;
    let key = Zeroizing::new(request.secret);
    let operation = uuid::Uuid::parse_str(&request.command_id)
        .context("invalid provider credential operation")?;
    ensure!(
        request.version == 1 && request.action == "instance.providers.apiKey.add",
        "invalid provider credential request"
    );
    authority.check()?;
    let receipt = install_api_key_account_owned_authorized(
        &authority.root,
        &authority.owner,
        operation,
        request.provider,
        key,
        || authority.check(),
    )
    .await?;
    authority.check()?;
    // Registry adoption remains the ordinary admitted adopt_native command.
    // Private local account IDs, references and fingerprints never leave holder.
    Ok(GuardedAuxiliaryResponse {
        result: json!({"action":request.action,"commandId":request.command_id,
        "configured":true,"adoptionRequired":true,"endpoint":receipt.endpoint,"models":receipt.models,
        "inferenceVerified":false}),
        publication: authority,
    })
}
pub(super) fn register(
    pool: &ctox_sync::native::NativePool,
    root: &Path,
) -> rxdb::rx_error::RxResult<()> {
    use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
    let weak = Arc::downgrade(pool);
    let root = root.to_path_buf();
    pool.register_guarded_auxiliary_request_handler(
        METHOD,
        Arc::new(move |peer, token, params| {
            let weak = weak.clone();
            let root = root.clone();
            Box::pin(async move {
                let checked_token = token.clone();
                let current = Arc::new(move || {
                    weak.upgrade().is_some_and(|pool| {
                        !pool.canceled.load(Ordering::SeqCst)
                            && pool.connection_handler.is_peer_current(&peer)
                            && pool
                                .connection_handler
                                .peer_capability_token(&peer)
                                .as_deref()
                                == Some(checked_token.as_str())
                    })
                });
                let authority = Authority::capture(root, token, current)
                    .map_err(|_| "provider credential ingress denied".to_owned())?;
                handle(Arc::new(authority), params)
                    .await
                    .map_err(|_| "provider credential ingress failed, denied or changed".to_owned())
            })
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token(root: &Path, role: &str) -> String {
        store::issue_business_os_capability_token_for_managed_user(
            root,
            "api-operator",
            "API operator",
            role,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap()
        .0
    }
    #[test]
    fn actual_signed_owner_and_admin_authority_is_required_and_rechecked() {
        for role in ["chef", "admin"] {
            let root = tempfile::tempdir().unwrap();
            let credential = token(root.path(), role);
            let authority =
                Authority::capture(root.path().into(), credential, Arc::new(|| true)).unwrap();
            assert!(authority.check().is_ok());
            store::open_store(root.path()).unwrap().execute("UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='api-operator'", []).unwrap();
            assert!(authority.check().is_err());
        }
        let root = tempfile::tempdir().unwrap();
        let member = token(root.path(), "user");
        assert!(Authority::capture(root.path().into(), member, Arc::new(|| true)).is_err());
        assert!(
            Authority::capture(root.path().into(), "forged".into(), Arc::new(|| true)).is_err()
        );
        let owner = token(root.path(), "chef");
        assert!(Authority::capture(root.path().into(), owner, Arc::new(|| false)).is_err());
    }
    #[test]
    fn request_cannot_supply_owner_actor_or_arbitrary_endpoint() {
        let request = json!({"version":1,"action":"instance.providers.apiKey.add","commandId":uuid::Uuid::new_v4().to_string(),"provider":"kimi","secret":"synthetic-test-only"});
        assert!(serde_json::from_value::<Request>(request.clone()).is_ok());
        for field in ["owner", "actor", "endpoint"] {
            let mut forged = request.clone();
            forged[field] = json!("untrusted");
            assert!(serde_json::from_value::<Request>(forged).is_err());
        }
    }
}
