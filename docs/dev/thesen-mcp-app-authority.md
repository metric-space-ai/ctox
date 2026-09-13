# THESEN MCP app authority repair

Source-only repair, based on `952838429` (origin/main). No tenant app/store edits,
deployment, native build or local compilation performed by this package.

## Root causes and boundary

`mcp_channel::modify_app` used `record_command(TrustedLocal)` after the MCP
permission gate. Its actor JSON included the gateway role, but
`store::rxdb_session_from_command` correctly does not trust that JSON: the
`trusted_rxdb_command_user` lookup reduced an unprovisioned gateway actor to
`user`. Admission therefore denied `apps.modify`. Merely fixing that first
gate would still fail on lease: native authorization revalidation previously
required a local active user row for every receipt.

App create/modify now pass a non-deserializable `AuthenticatedMcpAppCommand`
alongside the command, only for the existing authenticated managed admin/owner
channel. Native app and queue permission checks still run. The protected core
command aggregate stores the non-secret authority context in its existing
native authorization receipt. Lease revalidation restores authority only from
that canonical receipt, checks actor/role/permission/module binding, rechecks
current MCP channel/tool/actor/workspace/module policy and native permissions,
and rejects an inactive or differently privileged native user if one exists.
No synthetic user is created, no caller role/native_authorization JSON is
accepted, and no browser/RxDB capability gate is changed. The typed context's
trusted-role fields also cannot be deserialized from JSON. Gateway identity
and role are selected together, never mixed with caller `_context`.

This is a durable delegation, not storage of a bearer token. Gateway token
expiry prevents new gateway requests; it does not retroactively cancel an
already admitted command. Core does not remotely introspect gateway tokens at
lease. Current native/MCP policy can revoke execution, including disabling the
channel, excluding the actor/module, or deactivating/downgrading its native
account. Parent must review that lifetime contract before deployment.

Separately, `module_manifest_path`, `resolve_module_source_root`, and
`app_root_for_module_manifest` omitted `local-modules`, although
`module_manifest_loader::load_module_manifests` advertises them. The resolvers
now include local modules after bundled/installed candidates and preserve the
exact chosen manifest. IDs, manifest identity, namespace/module/manifest
symlinks, root containment and existing customer-binding policy are checked.
File symlinks are excluded from source projections, preventing source reads
outside the selected module. Existing malformed or symlink-based installations
may now fail closed and require explicit operator repair.

## Verification and integration

Eight tests on Linux (six on non-Unix), all under filter `mcp_app_authority`:

- gateway admin and owner survive native admission and lease without a local
  user; channel revocation denies the lease;
- spoofed JSON/context/native receipt and tokenless replicated commands deny;
- gateway actor/workspace binding, module scope revocation, inactive native
  account, downgrade and unsupported managed user role deny;
- local-module file listing/read and exact runtime root resolve;
- traversal, mismatched manifest and unbound customer package deny;
- namespace/module/manifest symlinks deny (Unix);
- symlinked file reads and relative path escapes deny (Unix);
- bundled, installed, local source precedence is unchanged.

Parent-owned THESEN run (two workers, shared native verification lock):

```sh
cargo test --release --locked --bin ctox --jobs 2 -- mcp_app_authority --test-threads=2
```

Recommended existing guard filters on the same native build: `app_source_`,
`gateway_managed_`, `capability_epoch_revokes_tokens_after_role_or_grant_change`,
and `queue_command` (inspect discovered tests before treating an empty filter
as evidence). No native test pass is claimed here. Formatting only was run with
`greppy bash-smart -- rustfmt --edition 2021 --config skip_children=true ...`.

PR139 at `59d7ed642` is not part of this base; store hunks are in source
resolution/admission/authorization/audit, not its secret-generation dispatch.
No changes to `secrets.rs`, `command_plane.rs`, tenant files, runtime data, or
Copernicus's `leadfeeder.rs` package. Native adapters are web-stack scrape
targets; historical shared queue-lease failures are not proved fixed by this
MCP app repair.

Remaining scope: existing MCP app-development response paths still describe
runtime-installed targets; this package fixes native admission and source
resolution, not that authoring/delivery contract. Parent must validate the
actual target and lifecycle before delegating edits to an operator-owned local
app. App validation/deployment and all adapter verification remain parent-owned.
