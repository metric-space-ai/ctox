# Managed upgrade: service cutover

Field trigger (Thesen, 2026-09-08 10:36–10:37 UTC): source preparation
published the new public executable and CTOX_ROOT wrapper before `current`
changed. A restart/watchdog launch could run the new release while manifest
and current still named the old release; the subsequent 15-second stop failed.

`install.sh --rebuild` now prepares only release-local launch binaries and
wrappers. Runtime dependency preparation remains idempotent. Only the Rust
updater publishes managed/public launchers after the guarded stop and atomic
current switch. A failed build therefore cannot change what the watchdog starts.

During update/rollback cutover the updater stops the watchdog timer first,
then any dispatched watchdog oneshot. Failure to stop either prevents cutover.
Unit refresh can enable the timer for future boots but cannot start it inside
this scope. The prior active timer is restored on ordinary success/error;
intentionally inactive timers remain inactive. A hard kill of the updater
cannot run destructor recovery; service/timer status must be checked on that
recovery path. No permanent mask and no new environment toggle is introduced.

`ServiceLifecycleTimeouts` owns the 300-second cold-start window and the
15-second ordinary stop budget. A release-switch stop uses their sum (315 s).
The generated Linux unit uses that same stop limit. Existing old units retain
their previously installed timeout until refreshed. Systemd stop is submitted
with `--no-block`; the native residue poll owns waiting, rather than the
five-second systemctl client timeout. A remaining process/socket/backend still
fails the switch; leased app creation/modification tasks still refuse a stop.

First-rollout caveat: the new shell preparation behavior takes effect when an
old updater builds this source. Rust lifecycle/watchdog changes run only when
the invoking updater binary includes them. No live Thesen upgrade or restart
was performed as part of local validation.

Checks:
- `tests/install_rebuild_cutover_smoke.sh`: real staging and wrappers, mocked
  expensive provisioning, old public-wrapper watchdog launch, failed build,
  current/manifest/desktop launcher preservation. Fails on base 365927a3c
  because the public wrapper differs, passes on the fix.
- Watchdog guard: stop order, in-flight oneshot stop refusal, error restoration,
  inactive/absent preservation, bounded control subprocess timeout.
- Lifecycle budget follows custom startup/shutdown durations.
- Existing install/app-task stop guard and rollback tests run in Crew liveness
  CI, together with cargo check/test compilation and Clippy.
- Existing shell-asset smoke fails both on base 365927a3c and this change:
  its synthetic source lacks scripts/assert-customer-app-isolation.mjs.
  This fixture defect is recorded, not bypassed.
