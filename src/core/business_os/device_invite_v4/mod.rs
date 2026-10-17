//! E54.4 core callback for the P4 owner's pairing-only control port.
//! This module does not register an unauthenticated RPC or open any data lane.
mod store;
use super::{
    launch_context_v4::{self, V4LaunchContext},
    store::BusinessOsUser,
};
use crate::service::store_inputs::StoreInputs;
use std::sync::Arc;
pub(crate) use store::{Challenge, Invitation, Redeem, Reject, Renew, RenewReject};

/// Called only from the authenticated invite.create command with its retained
/// native principal. The request payload cannot choose another user or role.
pub(crate) async fn create(
    owners: &Arc<StoreInputs>,
    principal: &BusinessOsUser,
    ttl_ms: i64,
) -> Result<Invitation, Reject> {
    owners
        .instance_signer()
        .map_err(|_| Reject::DeviceRejected)?;
    let invites = store::Invites::attach(owners.sync.writer.clone()).await?;
    let mut result = invites
        .create(principal.id.clone(), principal.role.clone(), ttl_ms)
        .await?;
    result.instance_display_name = launch_context_v4::instance_display_name(owners)
        .await
        .map_err(|_| Reject::DeviceRejected)?;
    owners
        .instance_signer()
        .map_err(|_| Reject::DeviceRejected)?;
    Ok(result)
}

/// P4 owns transport peer/generation fencing and publication to that exact
/// control connection. No routing label, callback body or secret grants a data
/// lane here: the returned E41 context must still redeem its normal E50 ticket.
pub(crate) async fn redeem(
    owners: &Arc<StoreInputs>,
    request: Redeem,
) -> Result<V4LaunchContext, Reject> {
    owners
        .instance_signer()
        .map_err(|_| Reject::DeviceRejected)?;
    let invites = store::Invites::attach(owners.sync.writer.clone()).await?;
    let prepared = invites.prepare(request).await?;
    let user = BusinessOsUser {
        id: prepared.principal.id.clone(),
        role: prepared.principal.role.clone(),
        display_name: prepared.principal.display_name.clone(),
        active: true,
        profile: serde_json::json!({}),
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    // Public E41 builder mints/signs the whole offer and registers E50 itself.
    // Failed minting does not consume the invitation. A losing final CAS never
    // publishes its unused ticket (which expires in the existing E50 owner).
    let context = launch_context_v4::build_for_device(owners, &user, prepared.device_pubkey())
        .await
        .map_err(|_| Reject::DeviceRejected)?;
    if context.v4.actor.id != user.id || context.v4.actor.role != user.role {
        return Err(Reject::DeviceRejected);
    }
    let rxdb::plugins::replication_webrtc::index_mod::hello::HelloFrame::Hello(hello) =
        &context.v4.hello.offer
    else {
        return Err(Reject::DeviceRejected);
    };
    invites
        .consume(prepared, hello.device_proof.device_id.clone())
        .await?;
    owners
        .instance_signer()
        .map_err(|_| Reject::DeviceRejected)?;
    Ok(context)
}

/// P4 sends this before the request and retains its nonce on that exact pairing
/// control connection. It must not accept another connection's challenge.
pub(crate) async fn challenge(owners: &Arc<StoreInputs>) -> Result<Challenge, RenewReject> {
    let instance = owners
        .instance_signer()
        .map_err(|_| RenewReject::DeviceUnknown)?
        .instance_id()
        .to_owned();
    store::Invites::attach(owners.sync.writer.clone())
        .await
        .map_err(|_| RenewReject::DeviceUnknown)?
        .challenge(instance)
        .await
}
/// Same P4 publication/generation fence as redeem. No data lane is opened here.
pub(crate) async fn renew(
    owners: &Arc<StoreInputs>,
    request: Renew,
) -> Result<V4LaunchContext, RenewReject> {
    let instance = owners
        .instance_signer()
        .map_err(|_| RenewReject::DeviceUnknown)?
        .instance_id()
        .to_owned();
    let invites = store::Invites::attach(owners.sync.writer.clone())
        .await
        .map_err(|_| RenewReject::DeviceUnknown)?;
    let prepared = invites.prepare_renew(instance, request).await?;
    let user = BusinessOsUser {
        id: prepared.principal.id.clone(),
        role: prepared.principal.role.clone(),
        display_name: prepared.principal.display_name.clone(),
        active: true,
        profile: serde_json::json!({}),
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    let context = launch_context_v4::build_for_device(owners, &user, &prepared.device_pubkey)
        .await
        .map_err(|_| RenewReject::DeviceUnknown)?;
    if context.v4.actor.id != user.id || context.v4.actor.role != user.role {
        return Err(RenewReject::DeviceRevoked);
    }
    let rxdb::plugins::replication_webrtc::index_mod::hello::HelloFrame::Hello(hello) =
        &context.v4.hello.offer
    else {
        return Err(RenewReject::DeviceUnknown);
    };
    invites
        .consume_renew(prepared, hello.device_proof.device_id.clone())
        .await?;
    owners
        .instance_signer()
        .map_err(|_| RenewReject::DeviceUnknown)?;
    Ok(context)
}
/// Authenticated devices-management adapter only, never an unpaired RPC method.
pub(crate) async fn revoke_device(
    owners: &Arc<StoreInputs>,
    device_id: String,
) -> Result<(), RenewReject> {
    owners
        .instance_signer()
        .map_err(|_| RenewReject::DeviceUnknown)?;
    store::Invites::attach(owners.sync.writer.clone())
        .await
        .map_err(|_| RenewReject::DeviceUnknown)?
        .revoke_device(device_id)
        .await
}
