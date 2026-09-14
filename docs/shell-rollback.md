# Native shell rollback targets

The native shell updater persists the selection displaced by activation in the
typed Business OS shell state. `rollbackTarget.kind` distinguishes:

- `no_previous_target`: no recorded rollback selection; rollback is rejected.
- `built_in_recovery`: activation displaced the native built-in shell selector.
- `slot`, with `version`: activation displaced a named signed shell slot.

The existing supported shell-update status includes `currentSlot`,
`previousSlot`, `rollbackSupported`, `rollbackTarget`, and
`rollbackRequiresSlotVerification`. `rollbackSupported` identifies this native
capability; it does not grant permission, assert that a target exists, or certify
a slot's current files. A slot target is fully checked again during rollback
(version syntax, release signature, compatibility, inventory and file hashes).
An invalid slot fails without changing the persisted selection; it never becomes
an implicit request for built-in recovery. `recoveryShell` describes the actual
`currentSlot == null` selector, rather than inferring it from `activeVersion`.

On first activation from the built-in selector, the new slot and the explicit
`built_in_recovery` rollback target are saved together before success is returned.
Rollback clears `currentSlot` and `activeVersion`, records the displaced slot as
the next rollback target, and retains all slot files. The normal native restart
then selects the built-in shell. Its status becomes `recovery` with health
`unknown`; this transition does not certify built-in assets as a verified slot.
The target selects the fallback of the installed native release, not a snapshot
of a previous native executable or its assets. Native release compatibility and
activation authorization remain separate requirements.

Legacy state with no `rollbackTarget` can recover only an explicitly stored
`previousSlot`. An absent previous slot stays `no_previous_target`, even when the
active version or current slot is absent. New explicit targets must agree with
the compatibility `previousSlot` field; unknown or inconsistent targets fail
closed. Old native executables do not implement built-in rollback and may drop
this additive field when rewriting state. Do not treat downgrade as preserving
this capability; verify supported status on the actual executor.

The `shell_recovery_*` native tests cover persisted activation and rollback
transitions, reopening the typed store, the built-in startup selector, legacy
state, missing/unsafe/bad-signature slot rejection and malformed target rejection.
They do not claim a live tenant activation or a successful download of a signed
release. The existing public activation rejection and signature tests retain the
release verification boundary. Shell release publication, staging and tenant
activation remain separate operations.
