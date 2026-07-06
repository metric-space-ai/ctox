# Business OS Roles, App Lifecycle And Operator Guide

This guide is business-facing operator guidance for CTOX Business OS. It does
not introduce a data path. Business data, app records, commands, files,
catalog state and runtime status stay on CTOX Sync Engine over RxDB/WebRTC.

## Roles

| Business label | Stored role | What it means |
| --- | --- | --- |
| Owner | `chef` | Owns the workspace, release policy, recovery and final role transfer decisions. |
| Admin | `admin` | Manages users, grants and operations, but does not assign the Owner role. |
| App-Verantwortliche:r | `founder` | Builds or manages assigned apps. Can manage only covered apps unless exact grants add more scope. |
| Teammitglied | `user` | Works with released or explicitly shared apps. Cannot modify apps by default. |

Compatibility aliases such as `owner` and `team` may be accepted at product
boundaries, but persisted storage stays `chef/admin/founder/user` for this
rollout.

## App Lifecycle Labels

Apps are visible according to lifecycle, audience and grants, not browser
storage.

| Label | Version/audience rule | Default visibility |
| --- | --- | --- |
| Privat | `0.x.y`, missing version or invalid SemVer | Only App-Verantwortliche:r and explicitly granted actors. |
| Vorschau | selected users with explicit app visibility | Only selected users and app managers. |
| Team | `1.0.0+` without extra restriction | Team-visible by default. |
| Eingeschraenkt | released but restricted audience | Only the selected audience and app managers. |

The version should be visible wherever an app is selected or launched. In the
Shell, lifecycle/version badges on app icons, tabs and App Store cards are the
operator signal for whether an app is private, previewed, team-visible or
restricted. Clicking the badge opens the governance view instead of launching
the app.

## App Changes

Routine edit actions such as `App aendern` are hidden for Teammitglied unless
an exact grant or assigned responsibility allows the action for that app.
Viewing source is separate from editing source: exact `apps.source.view` can
grant read-only source access without granting `apps.modify`.

High-value actions such as install, uninstall, release and rollback use native
Business OS policy. Hiding a button in the browser is never the only control.

## Publish And Review

The active publish path is the App Store `Freigeben` flow.

Before release, an operator checks:

- target version, normally promoting `0.x.y` to `1.0.0`;
- source snapshot and rollback target;
- responsible users;
- release notes;
- data review with read/write collections;
- locked data areas where the app must render a restricted state;
- final native catalog projection after publish.

The data review is evidence-only. It does not create hidden data grants. The
native release check requires explicit grants or declared locked-state
behavior for the reviewed data areas.

Rollback uses the release/version workflow and must restore the app version
without widening visibility or data access.

## Preview And Restricted Sharing

Preview and restricted audience are app-visibility decisions. They do not
grant source, edit, release, rollback or data access by themselves.

Use explicit app visibility grants for preview users. Do not treat
`apps.modify` or data grants as visibility grants.

## Agents And MCP

Humans and AI agents must see the same visible business scope before an action:

- actor and role;
- selected app and version;
- app lifecycle label;
- selected records or data areas;
- external-action state.

MCP module visibility is checked before data access. Data grants unlock data
only after app visibility passes. MCP external effects remain disabled for the
current rollout unless a later product phase adds an explicit approval model
and tests.

## Diagnostics

Use `Warum?` when an actor cannot see, open, edit, release, rollback or access
data through an app. The diagnostics view should explain business decisions
without raw policy JSON, prompt text, selected text, tokens or secrets.

Use `Support-Paket` when support evidence is needed. The support artifact is
support-safe and should include scope, redaction manifest, Activity summaries
and optional sanitized Why diagnostics, not raw record bodies or message
bodies.

## Recovery Boundaries

Use release rollback first for app-version problems. For permissions mistakes,
remove or deactivate the exact grant rather than deleting module
responsibility rows. Take a store backup before controlled database
maintenance.

For release/catalog projection drift, run
`ctox.module.repair_lifecycle_projection` with `dry_run: true` first. It lists
planned release/catalog projection actions without changing Business OS rows.
Apply it only after the dry-run scope is correct. This command is for
release-row/catalog projection repair, not for generic bad grants or source
snapshot restoration.

Do not use browser localStorage/sessionStorage to repair app visibility,
release state, audience state, tenant state or data grants. Browser storage is
only a UI/session hint and is not authoritative.

Run `ctox business-os backup restore-drill [--module <module-id>]` before
planned database maintenance or destructive rollout steps. The drill creates
raw sensitive backup material under `runtime/backup/business-os-drill-*`,
restores it into an isolated root and validates SQLite integrity, installed app
manifests, source snapshots, audit exports, release state, rollback target,
typed MCP policy and native RxDB catalog projection. Raw drill directories are
not support attachments; the manifest is signed with a CTOX Secret Store
HMAC-SHA256 key, includes local retention/expiry policy, declares downgrade and
cross-version restore policy, and records the AES-256-GCM portable snapshot
export created by the drill. That portable export is decrypted, hash-checked and
opened as a ZIP during the drill. Off-machine transfer must use the encrypted
portable artifact, never the raw snapshot directory, and the encryption key must
be escrowed through an organisation secret manager separate from the artifact.
Run `ctox business-os backup key-escrow-status` after the drill to confirm the
portable backup key exists in the CTOX Secret Store and to capture the key
fingerprint for the external escrow record; the command never prints the raw
key.
Use support-safe diagnostics, audit-retention exports or the gated
`ctox.business_os.backup.restore_drill` preflight artifact for support cases.
Before an active-root restore, run
`ctox business-os backup inspect-manifest --manifest <path>` against the
snapshot manifest. The preflight verifies the manifest signature with the local
Secret Store key, rejects unsupported manifest schema versions, blocks automatic
cross-version/downgrade restores and checks the encrypted portable artifact hash
without performing a destructive restore.
Use `ctox business-os backup prune-drills --dry-run` to inspect expired raw
drill directories, then `ctox business-os backup prune-drills` to delete only
expired `business-os-drill-*` directories whose manifest carries retention
metadata. Directories without retention metadata are reported, not deleted.

The current drill is not an automated destructive production-root restore
workflow. It includes a machine-readable active-root incident runbook with
quiesce, manifest hash/signature verification, compatibility verification,
restore-target and restart gates. Local
same-profile browser IndexedDB recovery is covered by the
`business-os-restore-resync-ui` Browser/Rust smoke: a browser-local write made
while the native peer is stopped stays local, then converges to native SQLite
over WebRTC after restart. Hosted/multi-workspace restore, cross-version or
downgrade behavior still need separate release evidence before those claims are
made. Portable/off-machine encryption is implemented by the drill; external key
escrow and approval remain operator gates.

Before a production release, the machine-readable release signoff in
`docs/business-os-security-privacy-signoff.json` and the human checklist in
`docs/business-os-production-release-signoff.md` must be `signed-off`, and the
release workflow must have a valid production smoke artifact.
