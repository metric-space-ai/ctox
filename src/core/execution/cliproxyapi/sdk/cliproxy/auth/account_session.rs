// Origin: CTOX
// License: AGPL-3.0-only
//! Host-owned durable account affinity. No credentials or prompt content enter this boundary.
use super::conductor_execution::AccountRoutingError;
use super::canonical_model_key;
use sha2::{Digest, Sha256};

/// The host must serialize first selection and failover across its router instances.
/// `choose` validates an existing binding against the current eligible accounts;
/// it may be called again after acquiring the host's writer fence. It never performs I/O.
pub trait AccountSessionStore: Send + Sync {
    fn choose(
        &self,
        key: &str,
        choose: &mut dyn FnMut(Option<&str>) -> Result<String, AccountRoutingError>,
    ) -> Result<String, AccountRoutingError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountSessionStoreError {
    Read,
    Write,
    InvalidRecord,
    Capacity,
}
impl std::fmt::Display for AccountSessionStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Read => "session binding read failed",
            Self::Write => "session binding write failed",
            Self::InvalidRecord => "session binding is invalid",
            Self::Capacity => "session binding storage is full",
        })
    }
}
impl std::error::Error for AccountSessionStoreError {}

/// Scope a client session to the configured provider and canonical model. Hash
/// the identifier instead of storing client labels or joining ambiguous strings.
pub fn account_session_key(provider: &str, model: Option<&str>, session: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    let model = canonical_model_key(model.unwrap_or_default());
    let session = session.trim();
    if provider.is_empty() || session.is_empty() || session.len() > 1024 {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(b"ctox.account-session.v1");
    for part in [provider.as_str(), model.as_str(), session] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    Some(format!("{:x}", digest.finalize()))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::api::account_selection::{self, ACCOUNT_SELECTION};
    use crate::sdk::cliproxy::auth::{AccountCandidate, AccountRouter, CooldownStateRecord, CooldownStateStore, CooldownStoreError, SchedulerPickOptions};
    use std::{collections::BTreeMap, sync::{Arc, Mutex}};

    #[derive(Default)]
    struct Sessions(Mutex<BTreeMap<String,String>>);
    impl AccountSessionStore for Sessions {
        fn choose(&self, key: &str, pick: &mut dyn FnMut(Option<&str>) -> Result<String, AccountRoutingError>) -> Result<String, AccountRoutingError> {
            let mut entries = self.0.lock().unwrap();
            let id = pick(entries.get(key).map(String::as_str))?;
            entries.insert(key.to_owned(), id.clone());
            Ok(id)
        }
    }
    #[derive(Default)]
    struct Cooldowns(Mutex<Vec<CooldownStateRecord>>);
    impl CooldownStateStore for Cooldowns {
        fn load(&self) -> Result<Vec<CooldownStateRecord>,CooldownStoreError> { Ok(self.0.lock().unwrap().clone()) }
        fn save(&self, states: &[CooldownStateRecord]) -> Result<(),CooldownStoreError> { *self.0.lock().unwrap()=states.to_vec(); Ok(()) }
    }
    fn candidates() -> Vec<AccountCandidate> {
        ["account-a","account-b"].into_iter().map(|id| AccountCandidate {
            auth_id:id.into(),provider:"claude".into(),weight:1,..Default::default()
        }).collect()
    }
    async fn pick(router: &AccountRouter, session: &str, accounts: &[AccountCandidate]) -> String {
        ACCOUNT_SELECTION.scope(Mutex::new(Default::default()), async {
            account_selection::request_session(&BTreeMap::new(), &serde_json::to_vec(&serde_json::json!({"session_id":session})).unwrap());
            router.select("claude",None,1_000,accounts).unwrap().auth_id
        }).await
    }

    #[tokio::test]
    async fn provider_session_standard_setup_keeps_healthy_account_across_priority_changes() {
        let sessions=Arc::new(Sessions::default());
        let router=AccountRouter::with_session_store(Arc::new(Cooldowns::default()),sessions.clone());
        let mut accounts=candidates();
        assert_eq!(pick(&router,"session-a",&accounts).await,"account-a");
        accounts[1].priority=100;
        assert_eq!(pick(&router,"session-a",&accounts).await,"account-a");
        assert_eq!(pick(&router,"session-b",&accounts).await,"account-b");
        let restored=AccountRouter::with_session_store(Arc::new(Cooldowns::default()),sessions);
        assert_eq!(pick(&restored,"session-a",&accounts).await,"account-a");
    }

    #[tokio::test]
    async fn provider_session_failover_stays_on_replacement_after_original_account_recovers() {
        let router=AccountRouter::with_session_store(Arc::new(Cooldowns::default()),Arc::new(Sessions::default()));
        let mut accounts=candidates();
        assert_eq!(pick(&router,"session",&accounts).await,"account-a");
        accounts[0].disabled=true;
        assert_eq!(pick(&router,"session",&accounts).await,"account-b");
        accounts[0].disabled=false;
        accounts[0].priority=100;
        assert_eq!(pick(&router,"session",&accounts).await,"account-b");
        assert_eq!(pick(&router,"different-session",&accounts).await,"account-a");
        accounts.retain(|account| account.auth_id!="account-b");
        assert_eq!(pick(&router,"session",&accounts).await,"account-a");
    }

    #[tokio::test]
    async fn provider_session_cooldown_failover_is_retained_after_limit_reset() {
        let cooldowns=Arc::new(Cooldowns::default());
        let router=AccountRouter::with_session_store(cooldowns.clone(),Arc::new(Sessions::default()));
        let accounts=candidates();
        assert_eq!(pick(&router,"session",&accounts).await,"account-a");
        cooldowns.save(&[CooldownStateRecord {
            provider:"claude".into(),auth_id:"account-a".into(),model:None,status:"cooling".into(),
            next_retry_after_ms:Some(2_000),reason:"quota".into(),quota:Default::default(),
            last_error:None,updated_at_ms:900,
        }]).unwrap();
        assert_eq!(pick(&router,"session",&accounts).await,"account-b");
        cooldowns.save(&[]).unwrap();
        assert_eq!(pick(&router,"session",&accounts).await,"account-b");
    }

    #[tokio::test]
    async fn provider_session_explicit_model_check_does_not_replace_chat_binding() {
        let router=AccountRouter::with_session_store(Arc::new(Cooldowns::default()),Arc::new(Sessions::default()));
        let accounts=candidates();
        assert_eq!(pick(&router,"session",&accounts).await,"account-a");
        ACCOUNT_SELECTION.scope(Mutex::new(Default::default()),async {
            account_selection::request_session(&BTreeMap::new(),br#"{"session_id":"session"}"#);
            account_selection::request_account(Some("account-b".into()));
            let exact=account_selection::candidates(&accounts);
            assert_eq!(router.select("claude",None,1_000,&exact).unwrap().auth_id,"account-b");
            // Manager-owned pins have the same isolation guarantee.
            account_selection::request_account(None);
            let options=SchedulerPickOptions { pinned_auth_id:Some("account-b".into()),..Default::default() };
            assert_eq!(router.select_with_options("claude",None,1_000,&accounts,&options).unwrap().auth_id,"account-b");
        }).await;
        assert_eq!(pick(&router,"session",&accounts).await,"account-a");
    }

    #[test]
    fn provider_session_unknown_limits_and_missing_session_do_not_round_robin() {
        let router=AccountRouter::with_session_store(Arc::new(Cooldowns::default()),Arc::new(Sessions::default()));
        for _ in 0..8 { assert_eq!(router.select("claude",None,1_000,&candidates()).unwrap().auth_id,"account-a"); }
    }

    #[tokio::test]
    async fn provider_session_request_identity_is_bounded_scoped_and_task_local() {
        let key=account_session_key("claude",None,"private-session").unwrap();
        assert_eq!(key.len(),64);
        assert!(!key.contains("private-session"));
        assert_eq!(Some(key.clone()),account_session_key(" CLAUDE ",Some("")," private-session "));
        assert_ne!(Some(key.clone()),account_session_key("codex",None,"private-session"));
        assert!(account_session_key("claude",None,&"x".repeat(1025)).is_none());
        ACCOUNT_SELECTION.scope(Mutex::new(Default::default()),async {
            let headers=BTreeMap::from([("x-Claude-Code-Session-ID".into(),vec!["private-session".into()])]);
            account_selection::request_session(&headers,br#"{"prompt_cache_key":"different"}"#);
            assert_eq!(account_selection::session_key("claude",None),Some(key.clone()));
            account_selection::request_session(&BTreeMap::new(),br#"{"prompt_cache_key":"private-session"}"#);
            assert_eq!(account_selection::session_key("claude",None),Some(key.clone()));
        }).await;
        assert!(account_selection::session_key("claude",None).is_none());
    }
}
