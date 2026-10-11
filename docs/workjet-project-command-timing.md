# Workjet guest project-list timing

The Business OS guest's `workjetProjectControl(request, diagnostics)` accepts an optional second argument with `onCommandTiming(sample)`. This is an in-process diagnostic observer, outside the project action payload. Project-list dispatch enables the existing `client_context.command_timing_probe`, then consumes the sample for its own generated command ID on success or failure. Concurrent requests have separate IDs; no global current-command slot or lifecycle event listener is used.

The sample contains only `commandId`, numeric `stagesMs`, and optional `pendingStage`/`pendingMs` for an unfinished preinsert wait. Complete roundtrips use `commandRoundtripStagesFromMarks`; failed roundtrips retain only intervals with both recorded boundaries. An absent stage is unknown, not zero. Native marks are required to attribute native intake, processing and browser observation. Push acknowledgement can overlap native intake, so the stage durations are not additive.

The desktop consumer must allowlist stage names, validate finite bounded durations and command IDs before IPC and before span annotation. Command payloads, capability tokens, actor/session data and arbitrary error messages are never sent to this observer. Observer exceptions do not change command success or failure. Existing callers and old guests retain their normal project-control behavior.

This instrumentation does not reset IndexedDB, replay commands, change authority, bypass the peer transport or claim installed acceptance. It must ship in the signed Business OS shell as well as the matching Workjet desktop bridge.
