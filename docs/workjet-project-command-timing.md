# Workjet guest project-list timing

The Business OS guest's `workjetProjectControl(request, diagnostics)` accepts an optional second argument with `onCommandTiming(sample)`. This is an in-process diagnostic observer, outside the project action payload. Project-list dispatch enables the existing `client_context.command_timing_probe`, then consumes the sample for its own generated command ID on success or failure. Concurrent requests have separate IDs; no global current-command slot or lifecycle event listener is used.

The sample contains only `commandId`, numeric `stagesMs`, and optional `pendingStage`/`pendingMs` for an unfinished preinsert wait. Complete roundtrips use `commandRoundtripStagesFromMarks`; failed roundtrips retain only intervals with both recorded boundaries. An absent stage is unknown, not zero. Native marks are required to attribute native intake, processing and browser observation. Push acknowledgement can overlap native intake, so the stage durations are not additive.

The desktop consumer must allowlist stage names, validate finite bounded durations and command IDs before IPC and before span annotation. Command payloads, capability tokens, actor/session data and arbitrary error messages are never sent to this observer. Observer exceptions do not change command success or failure. Existing callers and old guests retain their normal project-control behavior.

This instrumentation does not reset IndexedDB, replay commands, change authority, bypass the peer transport or claim installed acceptance. It must ship in the signed Business OS shell as well as the matching Workjet desktop bridge.

## Terminal tracking in a retained guest

The `command` project-list phase wraps the complete dispatch, including preinsert readiness and push; the phase alone does not prove insertion or native execution. The correlated sample distinguishes those boundaries. A complete preinsert sample plus push acknowledgement followed by a caller timeout means the guest is still waiting to observe the receipt; it must not be described as slow native processing without the native marks.

`shared/command-bus.js::waitForCommandState` serializes its status rebinds. Previously the first bind used an ordinary demand read, so it could join an earlier unresolved query window. Its held `rebindInFlight` then queued even revision-bound retries and generic master-change revalidation behind that read. Demand collections now start with a fresh exact-ID revision, using the existing loader's connection-generation and permission fences. Collections without a demand loader keep the previous local read path. Native failure, missing authority, finite retry/deadline and cleanup behavior remain enforced.

Reproduce the before/after wait on an isolated native protocol fixture:

```sh
PLAYWRIGHT_MODULE_PATH=/absolute/path/to/declared/playwright \
PLAYWRIGHT_BROWSERS_PATH=/absolute/path/to/browser/cache \
node src/apps/business-os/shared/workjet-command-native.browser.mjs \
  --native /absolute/path/to/v15_wire_daemon \
  --baseline /absolute/path/to/before-command-bus.js --output /absolute/path/to/result.json
```

The fixture uses real Chromium IndexedDB, a WebRTC DataChannel and the Rust SQLite query dispatcher. Its terminal receipt is seeded into the daemon's isolated `demo` collection; it does not run the Business OS project-list handler, test production enrollment/authorization, or certify installed Workjet. It deliberately holds only an older ordinary query in that fixture. No production document, token, pending write or checkpoint is read, reset or discarded. The fixture reports actual stage samples and native query durations separately; it never invents native timing marks.
