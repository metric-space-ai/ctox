//! Host adapter: only the foreground's held E22 signer and admitted native peer.
use super::*;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use sync_rights::{
    app_grant::{AppGrantSession, GrantPeerAuthority},
    app_grant_route::AppGrantRoute,
};
struct Entry {
    peer: Weak<BrowserPeer>,
    route: Arc<AppGrantRoute>,
}
static ROUTES: OnceLock<Mutex<HashMap<usize, Entry>>> = OnceLock::new();
struct Principal {
    admission: Weak<BrowserAdmission>,
    peer: Weak<BrowserPeer>,
}
impl GrantPeerAuthority for Principal {
    fn validate(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: &str,
        role: &str,
        device: &str,
        app_id: &str,
    ) -> Result<(), String> {
        let admission = self.admission.upgrade().ok_or("retired peer")?;
        admission.ensure_current().map_err(|_| "retired session")?;
        let peer = self.peer.upgrade().ok_or("retired peer")?;
        if peer.expires <= Instant::now()
            || peer.user != user
            || peer.role != role
            || peer.device != device
        {
            return Err("revoked".into());
        }
        validate_user(tx, user, role, peer.local).map_err(|_| "revoked principal")?;
        let table: String = tx
            .query_row(
                "SELECT table_name FROM _collections WHERE name='business_apps' AND state='active'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "app unavailable")?;
        let raw: String = tx
            .query_row(
                &format!(
                    "SELECT data FROM \"{}\" WHERE id=?1 AND deleted=0",
                    table.replace('"', "\"\"")
                ),
                [app_id],
                |r| r.get(0),
            )
            .map_err(|_| "app unavailable")?;
        let app: Value = serde_json::from_str(&raw).map_err(|_| "invalid app")?;
        if app["id"] != app_id || app["state"] != "active" {
            return Err("inactive app".into());
        }
        let assigned = app["assigned_user_ids"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id == user))
            || app["provenance"]["owner_id"] == user;
        let rights = sync_rights::read(tx).map_err(|_| "app rights unavailable")?;
        use crate::business_os::policy::{
            self, BusinessOsActor, BusinessOsPermission, BusinessOsScope,
        };
        if !policy::evaluate_with_rights(
            &rights,
            &BusinessOsActor::new(Some(user.into()), role),
            BusinessOsPermission::AppsView,
            &BusinessOsScope::module(app_id, assigned),
        )
        .allowed
        {
            return Err("app forbidden".into());
        }
        Ok(())
    }
}
pub(super) fn owner(
    inputs: &Arc<StoreInputs>,
    admission: &Arc<BrowserAdmission>,
    peer: &Arc<BrowserPeer>,
) -> RxResult<Arc<AppGrantRoute>> {
    let key = Arc::as_ptr(peer) as usize;
    let mut routes = ROUTES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| invalid())?;
    routes.retain(|_, e| e.peer.upgrade().is_some_and(|p| p.expires > Instant::now()));
    if let Some(entry) = routes.get(&key) {
        return Ok(entry.route.clone());
    }
    if routes.len() >= 256 {
        return Err(invalid());
    }
    let authority = admission.authority();
    let session = AppGrantSession::new(
        authority.clone(),
        inputs.instance_signer().map_err(|_| invalid())?,
        Arc::new(Principal {
            admission: Arc::downgrade(admission),
            peer: Arc::downgrade(peer),
        }),
        peer.user.clone(),
        peer.role.clone(),
        peer.device.clone(),
    )
    .map_err(|_| invalid())?;
    let route = Arc::new(AppGrantRoute::new(session, authority));
    routes.insert(
        key,
        Entry {
            peer: Arc::downgrade(peer),
            route: route.clone(),
        },
    );
    Ok(route)
}
pub(super) fn control(
    source: &Source,
    peer: &str,
    message: Value,
) -> BoxFuture<'static, RxResult<Option<Value>>> {
    let result = (|| {
        let inputs = source.inputs.upgrade().ok_or_else(invalid)?;
        let binding = source.admission.peer(peer).map_err(|_| invalid())?;
        owner(&inputs, &source.admission, &binding)
    })();
    Box::pin(async move {
        result?
            .handle(message, now() / 1000)
            .await
            .map_err(|_| invalid())
    })
}
pub(crate) async fn shell(
    admission: &Arc<BrowserAdmission>,
    inputs: &Arc<StoreInputs>,
    peer: &str,
) -> Result<String, StoreError> {
    let binding = admission.peer(peer)?;
    let route = owner(inputs, admission, &binding).map_err(|_| StoreError::Closed)?;
    route
        .shell_token(now() / 1000)
        .await
        .map_err(StoreError::Invalid)
}

pub(super) fn bound_owner(_admission: &Arc<BrowserAdmission>, peer: &Arc<BrowserPeer>) -> RxResult<Arc<AppGrantRoute>> {
    ROUTES.get().ok_or_else(invalid)?.lock().map_err(|_|invalid())?
        .get(&(Arc::as_ptr(peer) as usize)).filter(|e|e.peer.upgrade().is_some_and(|p|Arc::ptr_eq(&p,peer)))
        .map(|e|e.route.clone()).ok_or_else(invalid)
}
