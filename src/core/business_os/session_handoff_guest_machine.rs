// Origin: CTOX
// License: AGPL-3.0-only
//! Private native control: identifiers only, protected bytes remain in the native checkpoint store.
use super::*;

pub(super) async fn restore_machine<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    registry: Arc<super::super::super::super::NativeGuestRegistry>,
    binding: String,
    guest: String,
) -> anyhow::Result<CopyResponse> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (server, registry, binding, guest);
        anyhow::bail!("native machine restore requires Linux");
    }
    #[cfg(target_os = "linux")]
    {
        let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
        let live = lifetime.0.clone();
        let target = Arc::new(
            tokio::task::spawn_blocking(move || {
                Target::prepare(server, &binding, None, live, SessionHandoffPhase::Resume)
            })
            .await??,
        );
        let t = target.clone();
        let store = tokio::task::spawn_blocking(move || {
            t.current(&t.request, |_, _| {
                let path = t
                    .server
                    .gate
                    .root
                    .join("runtime/ctox-sync/received-checkpoints");
                private_dir(&path)?;
                let store = CheckpointStore::open(path, BLOB_LIMIT)?;
                reconstruction::verify_manifest(
                    &store.load(&t.request.checkpoint_digest)?,
                    &t.request,
                )?;
                Ok(store)
            })
        })
        .await??;
        let ready = registry
            .restore_received_machine(
                &guest,
                store,
                &target.request.binding_digest,
                &target.request.checkpoint_digest,
                target.as_ref(),
            )
            .await?;
        // Fresh native account/policy/copy/host fence after the readiness await.
        target.current(&target.request, |_, _| Ok(()))?;
        Ok(CopyResponse::GuestMachineRestored {
            checkpoint_digest: ready.import.checkpoint_digest,
            guest_id: ready.import.destination.guest_id,
            guest_service_session_id: ready.endpoint.guest_session_id,
            process_effect_id: ready.process_effect.effect_id,
        })
    }
}
