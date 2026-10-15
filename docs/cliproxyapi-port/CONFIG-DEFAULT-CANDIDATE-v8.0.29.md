# Candidate: preserve plugin configuration defaults

Upstream v8.0.29 (`5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3`,
`sdk/pluginapi/types.go:65-66`) adds `Default any` to `ConfigField`.
Previously Rust discarded that property during metadata decoding, so consumers
could not read plugin-declared defaults.

The Rust DTO now preserves the JSON value as `default_value`. Its wire name is
`Default`, matching the untagged Go field. The property is always serialized:
an omitted input or explicit null becomes `Default:null`, and false, zero,
empty strings, arrays and objects remain actual default values.

Eleven shared fixture cases exercise metadata round-trips. The qualification
also executes those cases against the actual pinned Go v8.0.29 type using a
Go overlay over the existing dependency checkout; the overlaid source is
byte-compared against the immutable upstream Git object before compilation.
This tests the changed wire contract, not a reimplementation of it in Go.

The injected management service and authenticated `GET /v0/management/plugins`
route project registered plugin metadata, including
`config_fields` at the entry and metadata levels, a logo, and lower-case
`default` values. The projection recursively HTML-escapes strings once,
matching `pluginMetadata` / `pluginConfigFields` in upstream's management
handler. Empty schema is `[]`; missing metadata and defaults are explicit
JSON null. Only the injected registered plugin's declared schema is used:
stored account/configuration values are never substituted. Unregistered
records cannot keep stale schema in the response.

Shared metadata fixtures compare these fields with reference Go structs and
projection functions extracted from the same immutable upstream source. This
reference is deliberately limited to metadata projection, not the full Go
management handler or plugin startup. The existing durable authority and
management authentication remain unchanged. Production host composition is
still a separate integration gap. The HTTP route is available only when an
explicit plugin service is attached; no service returns 404, unavailable
authorities return 503, and authentication runs before either authority is
read. The response includes `plugins_enabled` from the same durable snapshot
as the rows. This CTOX adapter does not invent a filesystem `plugins_dir`,
discover binaries or add plugin installation/configuration mutation routes.

This slice adds no model IDs, runtime fetches, environment settings or renderer
code. It does not promote the accepted upstream pin or establish complete
v8.0.29 compatibility.
