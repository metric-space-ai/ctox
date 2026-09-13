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

Ten tests on Linux (eight on non-Unix), all under filter `mcp_app_authority`:

- gateway admin and owner survive installed-app native admission and lease
  without a local user; channel revocation denies the lease;
- spoofed JSON/context/native receipt and tokenless replicated commands deny;
- gateway actor/workspace binding, module scope revocation, inactive native
  account, downgrade and unsupported managed user role deny;
- local-module file listing/read and exact runtime root resolve;
- traversal, mismatched manifest and unbound customer package deny;
- namespace/module/manifest symlinks deny (Unix);
- symlinked file reads and relative path escapes deny (Unix);
- bundled, installed, local source precedence is unchanged.
- local-app delegated modification rejects caller-supplied installed/source/
  local targets at both MCP and native admission, with no command/queue/shadow
  created and no local manifest change;
- a queued installed-app modification whose target becomes operator-owned
  local is rejected at lease, without recreating an installed shadow.

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

## Local authoring follow-up: fail closed, not a new target dialect

Delegated modification of operator-owned local apps is explicitly unsupported
and now rejected with `local_app_authoring_unsupported`, after authorization
and before queue creation. The same native guard runs on lease, protecting
already queued work if its selected source becomes local. Resolution uses the
checked native manifest/root, never request `install_target`, `app_directory`,
`source_root` or `development_contract`. Source listing/read remains available.
The allowed authority tests now use genuine installed modules; local-source
read tests remain local. No live local module is converted or installed.

This cannot safely be enabled by changing only MCP payload/response paths:

- `store::business_os_app_command_target_metadata` and
  `business_os_app_command_target_prompt_block` encode installed vs source only;
- `service_business_os_app_authoring::business_os_app_module_target_from_metadata`
  maps every non-installed target to `--source`;
- `configure_business_os_app_file_system_scope` only reserves and grants a
  writable installed-module directory, explicitly creating that directory;
- validator modes and completion metadata need an agreed local-source target,
  sandbox grant, version/ownership and schema-refresh contract together;
- `module_manifest_loader::load_local_module_manifests` explicitly declares
  local apps operator-owned, editable but nondeletable, outside app-store
  lifecycle management.

A complete typed local authoring lifecycle is a separate package. This patch
does not grant broader runtime writes, invent an install target, or use a
source-mode fallback. Parent's native test/build/unit state is untouched;
the already running immutable `6b76bb38b` verification remains that revision,
not verification of this follow-up. All deployment/adapter checks remain
parent-owned.
