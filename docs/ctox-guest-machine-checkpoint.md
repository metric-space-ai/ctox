# Native guest machine checkpoint

The native retained desktop exposes the machine connection for issue #183:

1. Under the actual source lifecycle/controller guard, call
   `save_checkpoint_live(expected_live_endpoint, memory_file)`. It checks the
   real endpoint, retires desktop access, pauses/exports RAM, then requires
   successful QMP quit and observed successful child exit. Its opaque
   `QuiescedQemuCheckpoint` is not an external-effect reconciliation receipt.
2. Outside issuer/SQLite decision fences, consume that witness with
   `store(checkpoint_store, memory_file)`. It hashes the independently provisioned
   immutable RAW base, freezes and hashes the source's assigned writable overlay,
   and emits bounded ordered RAM/disk chunks plus `native-guest-machine.json`.
   Include ALL returned entries in the SAME protected provider-state/history/
   effect manifest before publication. The manifest binds original guest,
   source process/endpoint, original service session, base, hardware resources,
   RAM and disk. Combined state stays below the existing 1 GiB transport budget;
   the base is not transferred as a checkpoint blob.
3. On the target, use `StagedQemuCheckpoint::stage(store, protected_entries,
   prepared_config, registered_guest_id, original_service_session,
   fresh_memory_file, fresh_disk_file)`. Inputs come from the actual completed
   protected import and native registration, never a renderer list. It verifies
   completeness, every CAS blob/full-content hash, native assignment, pinned
   profile/resources and the independently provisioned base. It returns no
   GuestImportReceipt or execution grant.
4. `RetainedQemuDesktop::spawn_checkpoint(&mut staged)` rechecks the retained
   base/disk identities and hashes before creating an incoming paused child.
   A failed attempt cannot replay. Retain this owner before awaiting
   `load_checkpoint(&mut staged)`; loading leaves it paused.
5. Architecture retains current account/policy/issuer/controller and real quorum
   admission, original Core session construction, pending/unknown effect
   rejection and the actual completed GuestImportReceipt. Only under freshly
   valid execution authority may it call `activate_restored`. This separately
   resumes QEMU and verifies the original live service session/capture.

The base/device profile is `pc-i440fx-5.1` with
`qemu64-v1,kvm=off,kvmclock=off,svm=off`, KVM kernel irqchip off or explicitly
admitted TCG. Changes to the device layout require a new checkpoint profile.

This is a native source integration. Byte-codec tests do not certify installed
guest readiness, effective revocation, original Core continuation or the
production two-host path. Goals 16/19 remain open until those are measured.

Architecture's source retirement callback `NativeGuestExecution::persist_source_journal`
now calls the actual retained Linux desktop export and includes its opaque witness's entries
in the same Core/history/workspace/effect manifest. The registry retains the exact child
before awaits, including failed or cancelled exports. Git capture, RAM/disk hashing and
chunk IO release worker/account/policy/controller publication locks; fresh native/quorum,
store identity and workspace revision checks precede checkpoint publication and writer-lease
release. Revocation retires the export synchronously without waiting for its IO mutex;
human stop waits for the actual child outside policy/controller guards.

This source callback still preserves unknown external effects and the registered process's
pending quorum effect. It neither creates source guest boot/admission nor activates the target.
The installed Darwin source contract and real two-host continuation remain unproved.
