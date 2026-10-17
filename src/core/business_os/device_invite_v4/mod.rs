//! E54.4 core callback for the P4 owner's pairing-only control port.
//! This module does not register an unauthenticated RPC or open any data lane.
mod store;
use super::{
    launch_context_v4::{self, V4LaunchContext},
    store::BusinessOsUser,
};
use crate::service::store_inputs::StoreInputs;
use std::sync::Arc;
pub(crate) use store::{Invitation, Redeem, Reject};

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
    let result = invites
        .create(principal.id.clone(), principal.role.clone(), ttl_ms)
        .await?;
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
    invites.consume(prepared).await?;
    owners
        .instance_signer()
        .map_err(|_| Reject::DeviceRejected)?;
    Ok(context)
}
