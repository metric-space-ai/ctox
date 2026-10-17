# xAI request thinking

The xAI Responses adapter uses the existing canonical request-thinking pipeline
for buffered and streaming execution and for the native preparation helper.
Unsupported `configuration_update` entries are removed after their latest
reasoning intent is resolved; explicitly supported private model capabilities
keep the entries and the baseline. Model suffixes and original summary intent
follow the canonical validation rules. Plugin normalization carries its actual
update decision, so deleted or rewritten updates cannot reappear from old input.

`XaiRequestOwner` retains the gateway's translator registry and thinking engine.
The host may supply its existing client processor through `with_processor`, then
bind the owner with `XaiExecutor::with_request_owner`. Selected capabilities come
from the typed private API-key model metadata, with options taking precedence
over request metadata. Extension JSON cannot grant configuration-update support.
The existing constructor creates isolated owners for native Responses callers;
there is no process-global plugin state, new runtime environment switch, model
catalog mutation or credential lookup in this preparation layer.

The corresponding upstream request and configuration-update tests are frozen at
[CLIProxyAPI 0d18d3b](https://github.com/router-for-me/CLIProxyAPI/blob/0d18d3b365be000eeaee7de4615f9da596b2059d/internal/runtime/executor/xai_executor_request.go).
This package does not promote the accepted upstream pin or establish complete
executor parity. Payload-finalizer, namespace-dispatch, speech and remaining
request/response changes still require their own semantic disposition. Runtime
account selection, credentials, transport ownership and installed Hi acceptance
remain at their existing boundaries.
