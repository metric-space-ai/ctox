# Native holding publication

A selected Supervisor's private holding controller must remain current while a response is physically sent, not only while its bytes are extracted. The guarded WebRTC responder composes its captured request connection/token and pool lifetime with this native issuer/Core/Policy guard on each physical poll. No guard is built from ConsumerFacts JSON, a computer label, or an execution Boolean.

`Arc<NativeSupervisorHoldingController>::publication_for(request, account_guard)` accepts the actual incoming `AdmittedConsumerAuthority`. It rejects a different root, transport object, connection generation, captured credential, or enrollment. It prepares the retained Core and Policy connections before polling. Attach the returned `Arc<dyn WebRTCPublicationGuard>` only to that request's `GuardedAuxiliaryResponse`.

The Models account guard implements:

```rust
trait NativeSupervisorPublicationCheck: Send + Sync {
    fn with_current(
        &self,
        scope: &NativeSupervisorCurrentPublication<'_>,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()>;
}
```

The scope has private fields and no constructor/Deserialize/Clone. It is issued only inside the current original controller's issuer/Core/Policy reservation. Its accessors are `controller() -> &Arc<NativeSupervisorHoldingController>`, `facts() -> &ConsumerFacts`, and `policy() -> &rusqlite::Connection`. It cannot outlive those borrowed reservations.

Models prepares its private account/configuration/secret metadata guard outside all these locks, verifies its exact original Arc and selected native account/model using this held policy view, and retains its own retirement fence across the single bounded publish callback. Do not enter another controller, Source transport, secret decrypt, database transaction, network operation or await from this callback. Neither OAuth nor private account fingerprints belong in the response. Missing or repeated callbacks are rejected.

The physical guard rechecks original native session expiry, Owner/project/thread/epoch, current command/confirmed plan lease and payload provenance, selected computer and Luma, current account/catalog/model, native selection seal, exact active controller and captured enrollment. A retired/replaced controller, changed account or stale Source generation cannot release buffered or streamed bytes.

The service may share `Arc<NativeSupervisorExecutionLease>` with `claim_shared`; this retains the same sealed native lease. It does not create a new execution or permit a retired controller to reclaim its unique original lease.

This change is an additive private integration boundary. Models' proxy, actual native Source responder, registered SDK session/turn/stop and installed Molecularity acceptance are separate obligations. It does not certify any SDK invocation or physical stop.
