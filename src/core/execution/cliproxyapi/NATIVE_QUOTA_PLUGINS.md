# Native management quota plugins

The instance management listener owns an optional process-isolated quota-plugin
runtime. It reads one revisioned document from the existing runtime SQLite
store. It never discovers executables, downloads artifacts, reads an ambient
proxy, or enables a plugin by default.

The local operator configures it with:

```
ctox runtime quota-plugins-configure /absolute/path/quota-plugins.json
```

The strict input has schema `ctox.cliproxyapi.quota-plugins.v1`,
`expected_revision`, `enabled`, and `plugins`. Each plugin supplies `id`, an
absolute `executable`, its lowercase SHA-256, optional `priority`, and optional
`config_secret`. A configuration secret must reference scope
`cliproxyapi-plugins` and name `<plugin-id>-config`; its YAML is resolved only
through the official encrypted credential provider. The runtime document holds
no plugin configuration values or OAuth tokens. At most four plugins are
accepted; an enabled configuration requires at least one. Disable with an empty
plugin list and the current expected revision, even if the old artifact is gone.
The CLI reports only the new revision, enabled state and plugin IDs.

The native listener starts only the explicitly pinned local artifacts and
requires each to register a quota provider. Quota capabilities attach to the
existing authenticated management routes. A runtime revision or private config
record change shuts down the old host and reloads it. Startup failure returns an explicit quota-runtime error while keeping other
management routes available, and suppresses further process starts until that
configuration identity changes. The instance management credential is required;
this feature never creates or rotates it.

The quota credential source resolves enabled Claude, Codex and Antigravity
accounts from the canonical explicit native topology. Its opaque private binding
covers account configuration and the current encrypted credential tuple. Both
topology writers and credential writers are fenced during final publication.
Disabled, deleted or replaced accounts cannot publish the captured result.
Inherited accounts outside this explicit topology and additional provider
adapters are not supplied by this source.

Plugins receive an account-bound HTTP capability, with no credential or binding
value serialized into their quota request. The native client injects the access
token only for the configured origin, does not follow redirects, rejects foreign origins,
removes caller-supplied authentication/cookies, disables ambient proxies, and
honors only the account's own protected proxy configuration. It uses a ten-second
request ceiling and an eight-MiB response ceiling, then revalidates the binding
before returning the result. Network IO does not hold the credential or topology
fence. The quota HTTP capability supports bounded unary responses. General
plugin executors, OAuth callbacks, streaming HTTP and routing reset integration
are outside this quota-only composition.

Process IPC is retained under the runtime store's `.qp` directory. Standard
LocalTransport path validation still applies; there is no TCP fallback. Private
plugin messages/configuration are never echoed by the native log callback; it
records only the plugin identity and a fixed level label.

The native regression lane explicitly builds the SDK fixture and runs
`native_quota_runtime_actual_process_uses_native_secrets_and_discards_retired_result`
with `--ignored`. Its `CTOX_QUOTA_TEST_PLUGIN_EXECUTABLE` input exists only in
`#[cfg(test)]` code. This isolated test covers the actual child process, native
secret resolution, scoped loopback HTTP, management authorization, replacement
rejection and shutdown. Synthetic fixture success is not live provider or
installed-product acceptance. No accepted upstream pin is promoted here.
