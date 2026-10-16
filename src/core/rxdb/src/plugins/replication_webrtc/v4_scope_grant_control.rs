//! E55 control-lane replies retain the existing native publication guard.
use super::*;
pub(super) async fn dispatch(source: &dyn V4StreamSource, peer: &str, request: Value, sender: &NativeSendScheduler, seq: &AtomicU64) -> RxResult<()> {
    if let Some(reply) = source.control(peer, request).await? {
        if !matches!(reply["type"].as_str(), Some("scope.grant" | "forbidden")) { return Err(invalid()); }
        let guard = source.publication(peer, &reply)?;
        let bytes = serde_json::to_vec(&reply).map_err(|_| invalid())?;
        sender.send(Lane::Control, WireSeq(seq.fetch_add(1, Ordering::Relaxed)), &bytes, Some(guard)).await?;
    }
    Ok(())
}
