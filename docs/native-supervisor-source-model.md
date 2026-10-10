# Selected Supervisor model requests from its Source

The private `ctox.workjet.project.supervisor.execution.v1` receiver adds two additive operations generated from `workjet-supervisor-source-v1.json`:

- `model_invoke`: original offer/controller UUIDs, retained operation UUID, `model_operation: messages|count_tokens`, raw SDK `body_json`, and `sdk_session_id` correlation.
- `model_read`: the same original IDs and operation UUID plus `sequence`, beginning at zero.

The server resolves Models' genuine `NativeClaudeLeaseModelProxy` from the exact retained native controller. Its random scoped model capability and provider OAuth stay native. The SDK may not override the account, model or upstream URL. A repeated operation with identical request/correlation returns the existing job; different contents fail. Core records the original operation before dispatch; a vanished process never silently invokes the same request again.

Invocation starts one owned Tokio task, bounded by the proxy's 300-second deadline and original controller/Source/account revocation. Cancel, offer pruning and handler drop retire the private account proxy and abort only its model tasks. The native SDK process is separate and requires its own real stop observation.

Reads deliver up to 32 KiB raw bytes as base64 with the real upstream HTTP status and streaming flag. Re-reading a sequence returns the same frame. Requesting exactly the next sequence acknowledges the previous frame; skipping or going backwards fails. A pending reply is neither an upstream failure nor completion. The queue is bounded at 8 MiB per operation and 64 operations per controller. Every physical response uses Models' retained account/config guard together with the original native Source/lease guard.

Native HTTP observations are persisted from Models' non-deserializable exchange after its actual response. A failed observation write prevents a successful terminal read. `sdk_session_id` and the upstream request ID are correlations only: they do not prove SDK start, turn, terminal result or stop. This bridge does not populate `actual_json`, accept caller-reported execution, activate the service producer, or fall back to the instance model. `execution_ready:false` remains until the real registered SDK/result path is connected.

Regression queue frames are isolated transport fixtures; they are not fabricated HTTP or installed Molecularity evidence.
