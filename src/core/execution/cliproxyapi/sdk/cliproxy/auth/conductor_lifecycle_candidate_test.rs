// ref: sdk/cliproxy/auth/conductor_lifecycle.go:145-278,400-401 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — real store/cache and stale-publication boundaries
// License: MIT (upstream); modifications AGPL-3.0-only
use super::super::{AuthStatus, ModelState};
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct Store {
    records: Mutex<BTreeMap<String, Auth>>,
    writes: AtomicUsize,
    fail: AtomicBool,
}
impl AuthStore for Store {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self.records.lock().unwrap().values().cloned().collect())
    }
    fn save(&self, auth: &Auth) -> Result<String, AuthStoreError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(AuthStoreError::Write);
        }
        // A durable record must not retain process-owned registration authority.
        let stored: Auth = serde_json::from_slice(&serde_json::to_vec(auth).unwrap()).unwrap();
        self.records.lock().unwrap().insert(auth.id.clone(), stored);
        Ok(auth.id.clone())
    }
    fn delete(&self, id: &str) -> Result<(), AuthStoreError> {
        self.records.lock().unwrap().remove(id);
        Ok(())
    }
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-04T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn auth() -> Auth {
    let mut auth = Auth::default();
    auth.id = "one".into();
    auth.provider = "meta".into();
    auth.status = AuthStatus::Active;
    auth.metadata
        .insert("access_token".into(), json!("dca:initial"));
    auth.metadata.insert("notes".into(), json!("original"));
    auth
}
fn setup() -> (Arc<Store>, AuthLifecycle, Auth) {
    let store = Arc::new(Store::default());
    let lifecycle = AuthLifecycle::new(
        store.clone(),
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(1),
    );
    let base = lifecycle
        .register(auth(), AuthMutationOptions::default(), now())
        .unwrap();
    (store, lifecycle, base)
}
fn mint(base: &Auth) -> Auth {
    let mut updated = base.clone();
    updated
        .metadata
        .insert("access_token".into(), json!("LLM|obsolete"));
    updated
        .metadata
        .insert("api_key".into(), json!("LLM|obsolete"));
    updated
}
fn publish(
    lifecycle: &AuthLifecycle,
    base: &Auth,
    updated: Auth,
    refresh: bool,
) -> Result<Option<Auth>, AuthLifecycleError> {
    if refresh {
        lifecycle.update_refreshed(base, updated, AuthMutationOptions::default(), now())
    } else {
        lifecycle.update_prepared(base, updated, AuthMutationOptions::default(), now())
    }
}

#[test]
fn candidate_removed_auth_is_never_recreated_by_delayed_prepare_or_refresh() {
    for refresh in [false, true] {
        let (store, lifecycle, base) = setup();
        let writes = store.writes.load(Ordering::SeqCst);
        assert!(lifecycle.delete("one").unwrap());
        assert!(publish(&lifecycle, &base, mint(&base), refresh)
            .unwrap()
            .is_none());
        assert!(lifecycle.get_cached("one").is_none());
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.writes.load(Ordering::SeqCst), writes);
    }
}

#[test]
fn candidate_same_id_registration_rejects_delayed_prepare_and_refresh_without_writing() {
    for refresh in [false, true] {
        let (store, lifecycle, base) = setup();
        let mut replacement = auth();
        replacement
            .metadata
            .insert("access_token".into(), json!("LLM|replacement"));
        let replacement = lifecycle
            .register(replacement, AuthMutationOptions::default(), now())
            .unwrap();
        assert!(replacement.registration_epoch > base.registration_epoch);
        let writes = store.writes.load(Ordering::SeqCst);
        assert_eq!(
            publish(&lifecycle, &base, mint(&base), refresh).unwrap_err(),
            AuthLifecycleError::StaleRegistrationEpoch
        );
        assert_eq!(store.writes.load(Ordering::SeqCst), writes);
        assert_eq!(
            lifecycle.get_cached("one").unwrap().metadata["access_token"],
            "LLM|replacement"
        );
        assert_eq!(
            store.list().unwrap()[0].metadata["access_token"],
            "LLM|replacement"
        );
    }
}

#[test]
fn candidate_reload_invalidates_prior_cycle_even_when_token_did_not_change() {
    let (store, lifecycle, base) = setup();
    assert_eq!(store.list().unwrap()[0].registration_epoch, 0);
    lifecycle.load(now()).unwrap();
    let current = lifecycle.get_cached("one").unwrap();
    assert!(current.registration_epoch > base.registration_epoch);
    let writes = store.writes.load(Ordering::SeqCst);
    assert_eq!(
        publish(&lifecycle, &base, mint(&base), false).unwrap_err(),
        AuthLifecycleError::StaleRegistrationEpoch
    );
    assert_eq!(store.writes.load(Ordering::SeqCst), writes);
    assert_eq!(current.metadata["access_token"], "dca:initial");
}

#[test]
fn candidate_delete_and_reregister_never_reuses_the_old_registration_epoch() {
    let (_, lifecycle, base) = setup();
    lifecycle.delete("one").unwrap();
    let replacement = lifecycle
        .register(auth(), AuthMutationOptions::default(), now())
        .unwrap();
    assert!(replacement.registration_epoch > base.registration_epoch);
    assert_eq!(
        publish(&lifecycle, &base, mint(&base), true).unwrap_err(),
        AuthLifecycleError::StaleRegistrationEpoch
    );
}

#[test]
fn candidate_mint_persistence_failure_never_installs_the_new_key() {
    for refresh in [false, true] {
        let (store, lifecycle, base) = setup();
        store.fail.store(true, Ordering::SeqCst);
        assert_eq!(
            publish(&lifecycle, &base, mint(&base), refresh).unwrap_err(),
            AuthLifecycleError::Store(AuthStoreError::Write)
        );
        assert_eq!(
            lifecycle.get_cached("one").unwrap().metadata["access_token"],
            "dca:initial"
        );
        assert_eq!(
            store.list().unwrap()[0].metadata["access_token"],
            "dca:initial"
        );
    }
}

#[test]
fn candidate_preparation_preserves_concurrent_settings_disabling_and_model_cooldown() {
    let (store, lifecycle, base) = setup();
    let mut current = base.clone();
    current.proxy_url = "https://new-proxy.example".into();
    current.metadata.insert("notes".into(), json!("user"));
    current.attributes.insert("weight".into(), "9".into());
    current.disabled = true;
    current.status = AuthStatus::Disabled;
    current.last_refreshed_at = now();
    current.model_states.insert(
        "muse".into(),
        ModelState {
            unavailable: true,
            ..ModelState::default()
        },
    );
    lifecycle
        .update(current, AuthMutationOptions::default(), now())
        .unwrap()
        .unwrap();
    let updated = publish(&lifecycle, &base, mint(&base), false)
        .unwrap()
        .unwrap();
    assert_eq!(updated.registration_epoch, base.registration_epoch);
    assert_eq!(updated.metadata["access_token"], "LLM|obsolete");
    assert_eq!(updated.proxy_url, "https://new-proxy.example");
    assert_eq!(updated.metadata["notes"], "user");
    assert_eq!(updated.attributes["weight"], "9");
    assert!(updated.disabled);
    assert_eq!(updated.status, AuthStatus::Disabled);
    assert!(updated.model_states["muse"].unavailable);
    assert_eq!(updated.last_refreshed_at, now());
    assert_eq!(store.list().unwrap()[0].proxy_url, updated.proxy_url);
}

#[test]
fn candidate_plain_update_rejects_old_epoch_but_accepts_current_legacy_snapshot() {
    let (_, lifecycle, base) = setup();
    let replacement = lifecycle
        .register(auth(), AuthMutationOptions::default(), now())
        .unwrap();
    assert_eq!(
        lifecycle
            .update(mint(&base), AuthMutationOptions::default(), now())
            .unwrap_err(),
        AuthLifecycleError::StaleRegistrationEpoch
    );
    let mut legacy = auth();
    legacy.label = "User edit".into();
    let updated = lifecycle
        .update(legacy, AuthMutationOptions::default(), now())
        .unwrap()
        .unwrap();
    assert_eq!(updated.label, "User edit");
    assert_eq!(updated.registration_epoch, replacement.registration_epoch);
}

#[test]
fn candidate_preparation_cannot_publish_under_a_different_account_identity() {
    let (store, lifecycle, base) = setup();
    let mut updated = mint(&base);
    updated.id = "another".into();
    let writes = store.writes.load(Ordering::SeqCst);
    assert_eq!(
        publish(&lifecycle, &base, updated, false).unwrap_err(),
        AuthLifecycleError::InvalidAuthId
    );
    assert_eq!(store.writes.load(Ordering::SeqCst), writes);
    assert!(lifecycle.get_cached("another").is_none());
}

#[test]
fn candidate_registration_epoch_overflow_fails_before_any_durable_write() {
    let store = Arc::new(Store::default());
    let lifecycle = AuthLifecycle::new(
        store.clone(),
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(1),
    );
    let mut impossible = auth();
    impossible.registration_epoch = u64::MAX;
    assert_eq!(
        lifecycle
            .register(impossible, AuthMutationOptions::default(), now())
            .unwrap_err(),
        AuthLifecycleError::RegistrationEpochExhausted
    );
    assert_eq!(store.writes.load(Ordering::SeqCst), 0);
    assert!(store.list().unwrap().is_empty());
    assert!(lifecycle.is_empty());
}
