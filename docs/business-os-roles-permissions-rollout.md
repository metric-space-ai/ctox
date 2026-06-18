# Business OS Roles and Permissions Rollout Guide

## Scope

This guide covers the CTOX Business OS roles and permissions rollout for the
current schema-stable implementation. It is operator guidance, not a new data
path.

Customer-facing guidance lives in `docs/business-os-app-access-and-roles-guide.md`.
Detailed operator guidance lives in
`docs/business-os-roles-permissions-operator-guide.md`. The blocking release
signoff artifact lives in `docs/business-os-security-privacy-signoff.json`;
`docs/business-os-production-release-signoff.md` is the human-readable
checklist companion. The release docs dry-run artifact is written to
`runtime/build/business-os-release-docs-dry-run.json`.

The rollout keeps the Business OS data plane RxDB/WebRTC-only. Browser actions
continue to write `business_commands`; the native peer evaluates commands,
writes status back, and records native audit activity in the existing
`business_events` table.

## Current Role Contract

Persisted role values remain:

- `chef`
- `admin`
- `founder`
- `user`

Business-facing labels map to:

- `chef` -> Owner
- `admin` -> Admin
- `founder` -> App-Verantwortliche:r
- `user` -> Teammitglied

Compatibility aliases are accepted at the boundaries:

- `owner` -> `chef`
- `business_os_admin` -> `admin`
- `team` -> `user`
- `business_os_team` -> `user`
- `business_os_user` -> `user`

No stored role-value migration is required for this rollout while storage stays
on `chef/admin/founder/user`.

## Permission Data

The rollout uses three existing/native data shapes:

- `business_users` for persisted users and roles.
- `business_module_acl` for founder-only module responsibility.
- `business_permission_grants` for additive, explicit grants.

The browser receives effective permission hints through the existing
`business_module_catalog.governance.permission_model` projection. No new RxDB
collection is introduced for this slice.

## Audit Activity

Native Business OS audit activity is recorded in `business_events`.

Covered event families:

- denied native policy decisions
- allowed native policy decisions from existing policy gates
- role changes
- app-responsibility changes
- Outbound approval decisions

MCP-specific activity remains in `business_os_mcp_events`. Do not merge these
stores operationally unless a later migration explicitly does so.

MCP policy, including audit retention, is stored as typed Business OS payload
`business_os.mcp_policy.v1`. Operators should change it with
`ctox business-os mcp policy set`; legacy `CTOX_BUSINESS_OS_MCP_*` runtime-env
values are read only as migration fallback until a typed policy exists.

The Settings Activity tab reads native `business_events` through the existing
`ctox.business_os.audit.list` command path and is gated to Owner/Admin-level
management rights.

For native audit retention, use `ctox.business_os.audit.retention` through the
same Business OS command path. The command requires `users.manage`, writes a
support-safe `ctox.business_os.audit_retention_export.v1` artifact under
`runtime/business-os/audit-exports` before it deletes expired `business_events`,
and redacts prompt, selected text, message body, raw payload, token and secret
fields. Do not manually delete `business_events` rows without a prior retained
export.

Outbound approval decision events intentionally store decision metadata and
selected record snapshots, but not message body text.

## Rollout Steps

1. Deploy backend and browser code together.
2. Verify existing users still appear with business-facing labels in Settings.
3. Verify App Store and Shell app actions show or hide based on projected
   permissions.
4. For one internal workspace, assign one explicit grant and verify only that
   user gains the intended action.
5. Trigger one denied Teammitglied app-management action and confirm it appears
   in Settings Activity for Owner/Admin.
6. Trigger one allowed Owner/Admin management action and confirm it appears in
   Settings Activity without creating repeated Activity-list self-events.
7. Trigger one role or app-responsibility change and confirm it appears in
   Settings Activity.
8. Trigger one Outbound approval decision and confirm it appears in Settings
   Activity without message body text.
9. Expand grants and module responsibility only after the Activity stream is
   reviewed.

## Release Checks

Minimum focused checks for this rollout:

```sh
cargo test --bin ctox business_os::policy
cargo test --bin ctox audit_list_command
cargo test --bin ctox business_event_audit
node src/apps/business-os/shared/roles.test.mjs
node src/apps/business-os/shared/permissions.test.mjs
node src/apps/business-os/shared/react-settings.test.mjs
node src/apps/business-os/shared/shell-permissions-ui.test.mjs
node src/apps/business-os/scripts/assert-permissions-ui.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
node src/apps/business-os/scripts/assert-production-release-docs.mjs
node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs
```

Run broader Business OS/RxDB guard suites before a release build:

```sh
cargo test --bin ctox business_os
node src/apps/business-os/rxdb/tests/run-all.mjs
```

## Recovery

Use normal release rollback first if the rollout changes UI behavior in a way
operators cannot accept. This rollout does not require a stored role-value
migration, so a release rollback does not need to rewrite `business_users.role`.

For permission-scope mistakes:

- Prefer removing or deactivating the specific explicit grant through the
  supported admin path available in the running build.
- Before controlled database maintenance or destructive rollout steps, run
  `ctox business-os backup restore-drill [--module <module-id>]` and keep the
  resulting manifest with the maintenance record.
- If a controlled database maintenance action is required, take a store backup
  first and change only the affected `business_permission_grants` row.
- Do not delete `business_module_acl` rows to fix generic grants; that table is
  still module-responsibility data.

For backup/restore readiness:

- The CLI drill writes raw backup material below
  `runtime/backup/business-os-drill-*`. Treat this directory as sensitive
  production data: it can contain app source, records, audit exports and raw
  SQLite snapshots.
- The drill snapshots `runtime/ctox.sqlite3`,
  `runtime/ctox-secrets.sqlite3`, `runtime/business-os.sqlite3` and
  `runtime/business-os-rxdb.sqlite3` with SQLite online snapshot semantics,
  copies installed app roots, source snapshots and audit exports, restores them
  into an isolated `restore-root` and runs integrity plus restored-state
  validation there.
- The snapshot manifest carries `raw_backup_security` retention and support
  attachment policy, `restore_compatibility` same-version/downgrade policy and
  `manifest_integrity` HMAC-SHA256 evidence backed by the CTOX Secret Store
  signing key. It also carries `portable_encrypted_export` with the
  AES-256-GCM snapshot ZIP path, ciphertext hash, chunk framing, decrypt/ZIP
  verification result and key metadata without secret value.
- Before active-root restore, run
  `ctox business-os backup inspect-manifest --manifest <path>`. The preflight
  checks manifest HMAC signature, supported schema version, same-version CTOX
  compatibility and encrypted portable artifact hash; cross-version and
  downgrade restore attempts remain blocked without release-level evidence.
- Run `ctox business-os backup key-escrow-status` after the restore drill to
  confirm the portable AES-256-GCM key exists in the CTOX Secret Store and to
  record the key fingerprint for the separate organisation secret-manager
  escrow. This status command does not reveal or export the raw key.
- Use `ctox business-os backup prune-drills --dry-run` before cleanup and
  `ctox business-os backup prune-drills` to remove only expired drill
  directories whose manifest carries retention metadata. Directories without
  retention metadata are reported but kept.
- The native command `ctox.business_os.backup.restore_drill` is only a
  support-safe preflight artifact under `runtime/business-os/restore-drills`.
  It is gated by `runtime.manage` and stores a sanitized command projection,
  but it does not create the raw backup.
- Do not attach raw `runtime/backup/business-os-drill-*` directories to support
  tickets. Off-machine transfer must use the encrypted portable export and a
  separately escrowed key; never send the key in the same channel as the
  artifact. Attach only support-safe diagnostics, audit-retention exports or
  the preflight restore-drill artifact unless the incident owner has approved a
  secure encrypted-backup transfer.
- Active production-root restore is still a manual incident procedure. The
  current drill validates an isolated restore and includes a machine-readable
  runbook/preflight with quiesce, manifest hash/signature verification,
  compatibility verification, portable-export verification, key-escrow
  confirmation, restore-target and restart gates. Local same-profile browser
  IndexedDB recovery after a native peer outage is covered by the
  `business-os-restore-resync-ui` Browser/Rust smoke. Hosted/multi-workspace
  restore and release-level downgrade compatibility still need separate release
  evidence before those claims are made; external key escrow remains an
  operator process.

For release/catalog projection drift:

- Run `ctox.module.repair_lifecycle_projection` with `dry_run: true` first.
  The result lists planned release/catalog projection actions without changing
  `business_records`, RxDB release rows or the catalog projection.
- Apply the same command with `dry_run: false` only after the dry-run output
  matches the intended module scope.
- This repair command restores release-row and module-catalog projections from
  native release state; it is not a general bad-grant or manifest-source
  rollback tool.

For audit-volume or audit-display issues:

- Keep `business_events` rows intact where possible; they are support evidence.
- Use `ctox.business_os.audit.retention` for planned audit pruning so the
  redacted export-before-prune artifact is written first.
- Hide or narrow the Activity UI in the release branch before deleting audit
  data.
- `ctox reset process-mining` is unrelated to Business OS `business_events` and
  must not be used as Business OS audit recovery.

For MCP policy issues:

- Use MCP policy disablement or allowlist narrowing, not native
  `business_events` edits.
- Use `ctox business-os mcp policy set`, which writes
  `business_os.mcp_policy.v1`; do not introduce new process-env production
  toggles.
- MCP audit evidence remains in `business_os_mcp_events`.

## Ship Criteria

The rollout is ready for a target workspace when:

- existing Owner/Admin access is unchanged
- Teammitglied cannot modify apps without an explicit grant
- assigned App-Verantwortliche can manage only covered apps
- Settings uses business-facing labels only
- Activity shows denied and allowed actions, role/app-responsibility changes and
  Outbound approval decisions
- RxDB-only guard passes
- no new HTTP Business OS data bridge was added
- customer/operator guidance matches current labels and release boundaries
- `runtime/build/business-os-release-docs-dry-run.json` is present in release
  evidence and links docs to current UI-source anchors and the smoke summary
- `docs/business-os-security-privacy-signoff.json` is `signed-off` for release
  tags with matching source hashes, and the human checklist in
  `docs/business-os-production-release-signoff.md` is checked
