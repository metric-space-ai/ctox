# Business OS Dynamic Apps, Versions and Permissions Concept

Status: Core implementation slice plus Phase 9 native lifecycle projection
locally production-ready on 2026-06-17; Phase 10D1/10D2 backend release
consistency, Phase 10E1 native release audit plus Settings Activity
Browser/Rust evidence, Phase 10E2 native release projection backend, Phase
10E3 UI/static projection consumption and Phase 10G Settings fallback
sub-slices, the Phase 10F App Store release UI/payload and live Browser/Rust
release proof, Phase 13A DB-isolation inventory, Phase 13B real Shell
guarded-DB path and Phase 13D DB-access drift guard
locally verified on 2026-06-17. Phase 11D launcher/start-menu lifecycle badges
and manager vs read-only lifecycle drawer states are also locally verified by
the Dynamic Apps Browser/Rust smoke. Phase 12A MCP app visibility/data split is
locally verified: MCP module listing, links, module detail/proposal paths and
execution now evaluate app lifecycle visibility separately from data grants.
Phase 12B/12C first browser slice is also locally verified for global
right-click and command context: the Shell CTOX context menu shows actor,
selected app/version/lifecycle, selection, data summary and external-action
state before submit; the same visible scope is submitted in `client_context`;
the command bus canonicalizes app/scope aliases; Coding Agents include
provider/workspace/session external-scope context; Business Chat scheduled
commands preserve existing `contextMeta.client_context`; Business Chat renders
preserved visible scope rows in the chat window; App Store context chat renders
and submits selected-app visible scope.
Phase 14A production smoke-mode registry is also locally verified, so
release/audience/agent/auth/fresh-profile modes have fixed evidence contracts;
the release story, covered audience/dynamic apps stories and MCP backend
visibility split plus agent-scope slice are implemented,
and the agent-scope Browser/Rust proof now covers the real global right-click,
App Store context-chat and Business Chat rendered-scope paths. Native/MCP audit
metadata parity is also locally verified: native policy events keep redacted
scope-only `client_context`, MCP events keep `business_scope`, and tests prove
prompt, selected text and MCP payload content are not copied into audit
metadata. Phase 13E dynamic-app runtime safety is locally verified as an
explicit same-origin trusted-code contract: runtime-installed apps receive a
guarded `ctx.runtimeCapabilities` contract, the installed-app validator rejects
forbidden network fetch, dynamic import, browser storage, Shell-global,
cached-DB, Worker/navigation/evaluator and direct CTOX control-command bypasses,
and Browser/Rust dynamic-app smoke proves the real reload/openModule path sees
that contract. Phase 13F browser-storage scope is locally verified: Shell/App
Store UI preference storage is workspace/actor scoped where relevant, modules
receive `ctx.storageScope`, and Browser/Rust Dynamic Apps, Audience and Release
smokes prove storage is non-authoritative for app visibility, audience, release
state and data grants. Phase 13C now has a packaged/starter user-module batch
locally verified for `coding-agents`, `calendar`, `conversations`,
`buchhaltung`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `matching`, `notes`,
`outbound`, `research`, `shiftflow`, `spreadsheets` and `support`: the
guarded DB facade denies collection/property/raw/real-context access before `data.read`,
exposes matching `ctx.permissions` decisions, renders a Shell locked state for
Support without data grants, allows read after an exact collection grant and
denies writes without `data.write`. `coding-agents`, `calendar`, `buchhaltung`, `customers`,
`invoices`, `iot`, `matching`, `notes`, `outbound`, `research` and `shiftflow` are now smoked against their module-owned
`coding_agent_sessions`, `calendar_events`, `accounting_journal_entries`, `customer_accounts`,
`accounting_invoices`, `iot_widgets`, `matching_requirements`, `notes`, `outbound_campaigns`, `research_tasks` and `planning_shifts`
collections.
Phase 16A-16D release-gate work is locally verified and CI/release-wired:
Business OS declares audit-clean test-only `esbuild@0.28.1` and
`playwright@1.60.0` dependencies in `src/apps/business-os/package-lock.json`,
the bootstrap guard proves package/lock pins and module resolution, CI runs the
declared JS bootstrap/audit/tests/module bundles, the Browser/Rust smoke matrix
writes `runtime/build/business-os-smoke-matrix-summary.json` by default, and
the tag-release workflow has a `business-os-production-gate` job before any
release artifacts are uploaded. The smoke artifact validates schema
`ctox.business_os.smoke_matrix_summary.v1`, records git revision, URL,
auth/profile/role/tenant context, evidence keys, warning budgets and result,
and rejects final production summaries with missing context values or accepted
attempts above their recorded budgets. Legacy migration fixtures now cover
private `0.x`, missing version, invalid SemVer, released `1.x`, restricted and
preview legacy manifests without widening app visibility or data grants.
Remaining Phase 16 work before any full production-ready claim: actual
security/privacy signoff and final human customer/operator release review. The
customer/operator docs and machine-readable signoff structure now exist, but the
JSON signoff remains `pending-signoff` by design until reviewed.
The docs guard also writes `runtime/build/business-os-release-docs-dry-run.json`
with UI-source anchors and smoke-summary metadata; final human release review is
still required.
Phase 13 system raw cleanup has also removed direct `ctx.db.raw` access from
Browser, CTOX, Knowledge, Reports and Tickets. They now resolve their system
collections through `ctx.db.collection(name)`. Creator runtime and generated
app templates also no longer use collection-property or `ctx.db.collections`
fallback access. Phase 13G has also removed the remaining guarded-module
collection-property/proxy fallback paths from Buchhaltung, Calendar, Coding
Agents, Customers, CV Print Builder, Documents, Invoices, IoT, Notes, Outbound,
Shiftflow, Spreadsheets and Support. All 24 module inventory entries now report
raw/property/proxy/cached-handle flags as false, and the Dynamic Apps
Browser/Rust smoke passes clean for all 16 packaged guard modules. The
remaining App Store/Browser/Creator/CTOX/Desktop/Knowledge/Reports/Tickets
system/internal surfaces are now exact scoped exceptions through
`SCOPED_SYSTEM_MODULE_DB_COLLECTIONS`; the inventory stores matching
`scoped_collections` and the Dynamic Apps Browser/Rust smoke proves 8/8 scoped
modules with allowed access to their scoped collection, denial of registered
foreign collections through collection/property/raw/collections access,
permission-facade parity and capability-contract evidence. Phase 13H also
closes the previously unscoped Settings, Desktop-app,
Business Chat and Business Reporter facades with explicit collection allowlists;
the inventory guard now reports 0 unscoped facades, and live Browser/Rust
coverage proves Settings/Business Chat, Settings diagnostics/support, Desktop
File Viewer and broad Shell paths still work through those scoped facades.
Phase 13 is complete locally.
Phase 14 is locally closed for the current local-workspace product claim.
Auth-scope is verified through the real Browser/Rust path for login,
authenticated reload, logout, logged-out reload, protected-access blocking,
tenant-scope stability, forged stored pairing/auth data denial and the explicit
`local-workspace-only` tenant claim. Phase 14D/14E/14F fresh-profile,
visual-label and scale-budget coverage is also locally verified: a clean
profile loads authoritative projection, lifecycle/version labels, disabled
reasons, desktop and narrow viewport states without browser storage widening
access, and the scale fixture proves 57 catalog apps, 64 explicit grants, 96
release versions, 128 native audit events plus render/start-menu/App-Store
budgets. Hosted/multi-workspace isolation is not claimed by this local release
and remains future hosted-product scope. The same static required-mode guard is
wired into CI. Phase 15A Shell "Warum?" diagnostics UI
is now locally verified for the lifecycle drawer: manager and read-only app
states explain actor, app visibility/open/edit/source/release/rollback and
per-data-area read/write decisions with stable Browser/Rust evidence keys.
Phase 15A native and Settings "Warum?" diagnostics are also locally verified:
`ctox.business_os.why` returns native lifecycle/policy diagnostics for
visibility/open/edit/source/release/rollback and data read/write decisions,
including sanitized diagnostics command projections. Settings module management
dispatches this native command and the live `business-os-roles-permissions-ui`
Browser/Rust path proves its business-facing diagnostics render with required
actor/action/data rows and without raw policy keys, nested decision payloads or
prompt/token/selection leakage.
The native support diagnostics artifact slice is also locally verified:
`ctox.business_os.support.export_diagnostics` returns
`ctox.business_os.support_diagnostics.v1` with a `support-safe-v1` redaction
manifest, actor/scope, Activity summaries without raw event payloads and an
optional sanitized Why summary. Settings module management now exposes this as
a per-app `Support-Paket` action; the live `business-os-roles-permissions-ui`
Browser/Rust smoke proves visible schema/protection/scope/activity/why rows,
redaction and JSON download evidence without browser warnings/errors/404s.
Native `ctox.business_os.audit.retention` now covers the first
`business_events` retention slice: expired rows are exported to a support-safe
`ctox.business_os.audit_retention_export.v1` JSON artifact before optional
prune, with the command gated by `users.manage` and sanitized command
projection. Native retention defaults are now configurable through typed
Business OS state as `business_os.audit_retention_policy.v1`: the
`ctox.business_os.audit.retention_policy.set` command is `users.manage`
gated, stores a sanitized command projection, and `ctox.business_os.audit.retention`
uses that persisted policy when the request omits `retention_days`.
Native `ctox.module.repair_lifecycle_projection` now also supports a dry-run
recovery drill for release/catalog projection drift: operators can inspect
planned release/catalog projection actions before applying the repair, and the
persisted command projection is sanitized. The same command now has an
optional `repair_stale_grants` recovery path for active module-scoped grants
whose module no longer exists; dry-run reports the stale grant action and apply
deactivates only the stale grant while leaving valid grants untouched.
It also supports `repair_invalid_version_refs` for release snapshots whose
`source_version_id` or `rollback_version_id` no longer points to an existing
module source version; dry-run reports the invalid field and apply clears only
that broken reference before the release projection is regenerated.
`repair_orphan_private_apps` covers legacy/restore states where a private
runtime `0.x` app has no active App-Verantwortliche:r: dry-run shows the
recovery assignment, and apply assigns the current Admin/Owner actor through
the existing audited `business_module_acl` responsibility path.
The existing source-version rollback path is now explicitly tested for
manifest and source-file restore: `ctox.module.rollback_version` restores
`module.json`, editable source files and removes files that were added after
the target source version.
Native isolated backup/restore drill coverage now exists for dynamic apps:
`ctox business-os backup restore-drill [--module <id>]` snapshots the core,
CTOX Secret Store, Business OS and native RxDB SQLite stores, copies installed
app roots, source snapshots and audit exports, restores them into an isolated
root and validates release rows, rollback target, app manifests, source
snapshots, typed MCP policy, typed native audit-retention policy and RxDB
catalog projection. This protects the app lifecycle/versioning model during
maintenance. The snapshot manifest now includes `raw_backup_security` retention
and support-attachment policy, `restore_compatibility` same-version/downgrade
policy and `manifest_integrity` HMAC-SHA256 evidence backed by the CTOX Secret
Store signing key. It also writes `portable_encrypted_export`: a chunked
AES-256-GCM snapshot ZIP whose ciphertext hash, framing metadata, key reference
without secret value, decrypt/hash verification and ZIP-entry verification are
stored in the signed manifest. `ctox business-os backup prune-drills` reports
and removes only expired raw drill directories with manifest retention metadata.
The drill and support-safe preflight now include a machine-readable active-root
restore runbook with explicit quiesce, manifest hash/signature verification,
portable-export verification, key-escrow confirmation, compatibility
verification, restore-target and restart gates, but they do not overwrite the
active production root. The
separate `business-os-restore-resync-ui` Browser/Rust smoke now covers the
local same-profile browser IndexedDB case: a browser-local desktop file/chunk
write made while the native peer is stopped does not reach native SQLite until
the peer restarts, then converges over WebRTC with warning/error/request-failure
counts 0. The restore-drill artifact still exposes browser IndexedDB and hosted
WebRTC as drill-local `remaining_boundaries`, because the native drill itself
does not execute a browser. Hosted/multi-workspace WebRTC restore remains future
hosted-product evidence.
Remaining future product work: hosted/multi-workspace tenant boundary,
release-level cross-version/downgrade restore, external key-escrow signoff,
security/privacy signoff and final customer/operator rollout review.
MCP audit retention is no longer part of that remainder: it now uses typed
`business_os.mcp_policy.v1` policy state, while legacy `CTOX_BUSINESS_OS_MCP_*`
runtime-env values remain a migration fallback only.
The full production-readiness rollout is tracked in
`docs/business-os-roles-permissions-plan.md` Phases 10-16. That plan now
breaks the remaining work into independently testable slices for release audit
and gates, audience state, agent scopes, DB isolation, local tenant/auth proof,
fresh-profile storage/scale proof, observability/recovery, native backup drill,
hosted restore/cross-version backup evidence, security/privacy signoff and
customer/operator rollout docs.

Passed locally on 2026-06-18 for Phase 16A-16D release-gate hardening:

- `npm ci --ignore-scripts --prefix src/apps/business-os`
- `npm audit --audit-level=low --prefix src/apps/business-os`
- `npm --prefix src/apps/business-os test`
- `npm --prefix src/apps/business-os run test:module-bundles`
- `cargo build --locked --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
- `node --check src/apps/business-os/app.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/core/rxdb/tools/business_os_production_smoke_registry.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs src/apps/business-os/modules/ctox/test.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- Release workflow dependency guard returned
  `business_os_release_gate_upload_dependency=1`.
- Warning-clean Browser/Rust production matrix passed Release, Audience, Agent
  Scope, Auth Scope and Fresh Profile with browser warnings/request
  failures/startup reloads 0 and complete context fields in
  `runtime/build/business-os-smoke-matrix-summary.json`.
- `cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  proves the Phase 16E legacy fixture cases for private `0.x`, missing version,
  invalid SemVer, released `1.x`, restricted, preview and idempotent partial
  backfill behavior.

Passed locally on 2026-06-18 for the Phase 15A Shell "Warum?" diagnostics UI
slice, native diagnostics command slice, Settings diagnostics UI slice and
live Settings Browser/Rust proof:

- `node --check src/apps/business-os/shared/shell-permissions-ui.js`
- `node --check src/apps/business-os/app.js`
- `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs`
  - 9 passed.
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60851 SIGNALING_PORT=60852 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  - matrix OK with `business_os_dynamic_lifecycle_why_diagnostics_visible=1`,
    `business_os_dynamic_lifecycle_why_diagnostics_rows=1`,
    `business_os_dynamic_lifecycle_why_diagnostics_data=1`, browser
    warnings/errors/404/request failures 0 and
    `startup_smoke_hook_reload_count=0`.
- `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/service/service.rs`
- `cargo test --bin ctox business_os_why_command --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  - 2 passed, 0 failed, 1790 filtered out; existing warnings only.
- `node --check src/apps/business-os/shared/react-settings.js`
- `node --test src/apps/business-os/shared/react-settings.test.mjs`
  - 8 passed, 0 failed.
- `node --check src/apps/business-os/modules/desktop/index.js`
- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  - completed with existing warnings.
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60961 SIGNALING_PORT=60962 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  - matrix OK with
    `business_os_roles_permissions_settings_why_diagnostics_visible=1`,
    `business_os_roles_permissions_settings_why_diagnostics_rows=1`,
    `business_os_roles_permissions_settings_why_diagnostics_redacted=1`,
    browser warnings/errors/404/request failures 0,
    `startup_smoke_hook_reload_count=0` and
    `startup_smoke_hook_wait_ms=93`.
- `cargo test --bin ctox business_os_support_diagnostics_export --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  - 1 passed, 0 failed, 1792 filtered out; existing warnings only.

This closes the Shell/browser, native command, Settings UI and live Settings
Browser/Rust slices of Phase 15A plus the native and Settings support artifact
schema/redaction/export slices. Native `business_events` export-before-prune
retention now has a first command slice, but broader recovery and
hosted restore/cross-version/raw-backup requirements remain open in Phase 15.

Purpose: CTOX Business OS ist ein App-Betriebssystem. Apps werden im System
erstellt, angepasst, versioniert, veroeffentlicht und von Menschen oder KI-
Agenten genutzt. Rechte und Rollen muessen deshalb nicht nur "wer darf was"
beantworten, sondern auch "in welchem App-Lifecycle-Zustand ist diese App" und
"welche Daten darf diese App fuer diesen Actor sehen oder veraendern".

## Source-Validated Current State

Current source anchors:

| Area | Current source |
| --- | --- |
| Shell app visibility for runtime-installed apps | `src/apps/business-os/app.js::moduleAppearsInSwitcher`, `src/apps/business-os/app.js::listLaunchTargets`, `src/apps/business-os/shared/app-lifecycle.js::canSeeModuleForAppVersion` |
| App Store app visibility for runtime-installed apps | `src/apps/business-os/modules/app-store/index.js::rawCatalogItems`, `src/apps/business-os/modules/app-store/index.js::canSeeAppStoreModuleForAppVersion`, `src/apps/business-os/shared/app-lifecycle.js::canSeeModuleForAppVersion` |
| Shell launcher/start-menu item rendering | `src/apps/business-os/app.js::buildStartMenuItem` renders icon, label, pin and runtime-app lifecycle/version badge; badge click opens `openAppLifecycleDrawer` without launching the app |
| Shared app lifecycle projection | `src/apps/business-os/shared/app-lifecycle.js::appLifecycleState`, `src/apps/business-os/shared/app-lifecycle.js::appLifecycleBadge`, `src/apps/business-os/shared/app-lifecycle.js::canSeeModuleForAppVersion` |
| Shell switcher visibility gate | `src/apps/business-os/app.js::moduleAppearsInSwitcher`, `src/apps/business-os/app.js::listLaunchTargets` |
| Shared browser permission helper | `src/apps/business-os/shared/permissions.js::BusinessOsPermissions`, `src/apps/business-os/shared/permissions.js::canUseBusinessPermission` |
| Browser app modify/source affordances | `src/apps/business-os/app.js::showTargetContextMenu`, `src/apps/business-os/app.js::renderModuleAppBar`, `src/apps/business-os/app.js::canModifyModule`, `src/apps/business-os/app.js::canViewModuleSource` |
| Shell lifecycle "Warum?" diagnostics | `src/apps/business-os/app.js::openAppLifecycleDrawer`, `src/apps/business-os/shared/shell-permissions-ui.js::buildModuleWhyDiagnosticsView`, `src/apps/business-os/shared/shell-permissions-ui.js::renderModuleWhyDiagnosticsHtml`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsDynamicAppsUiSmoke` |
| Native "Warum?" diagnostics command | `src/core/business_os/store.rs::BusinessOsWhyDiagnosticsRequest`, `src/core/business_os/store.rs::business_os_why_diagnostics`, `src/core/business_os/store.rs::business_os_why_visibility_decision`, `src/core/business_os/store.rs::business_os_why_data_permission_decision`, `src/core/business_os/store.rs::accept_rxdb_business_command` |
| Settings "Warum?" diagnostics UI | `src/apps/business-os/shared/react-settings.js::loadModuleWhyDiagnostics`, `src/apps/business-os/shared/react-settings.js::nativeWhyDiagnosticsView`, `src/apps/business-os/shared/react-settings.js::moduleWhyDiagnosticsHtml`, `src/apps/business-os/shared/react-settings.test.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsRolesPermissionsUiSmoke` |
| Native support diagnostics artifact | `src/core/business_os/store.rs::BusinessSupportDiagnosticsExportRequest`, `src/core/business_os/store.rs::business_os_support_diagnostics_export`, `src/core/business_os/store.rs::support_diagnostics_activity_summary`, `src/core/business_os/store.rs::support_diagnostics_why_summary`, `src/core/business_os/store.rs::accept_rxdb_business_command` |
| Native audit retention export-before-prune | `src/core/business_os/store.rs::BusinessAuditRetentionRequest`, `src/core/business_os/store.rs::business_os_audit_retention_export`, `src/core/business_os/store.rs::business_events_before`, `src/core/business_os/store.rs::prune_business_events_before`, `src/core/business_os/store.rs::accept_rxdb_business_command` |
| Native lifecycle projection recovery | `src/core/business_os/store.rs::ModuleLifecycleProjectionRepairRequest`, `src/core/business_os/store.rs::repair_module_lifecycle_projections`, `src/core/business_os/store.rs::module_lifecycle_projection_repair_safe_command`, `src/core/business_os/store.rs::accept_rxdb_business_command` |
| Settings support diagnostics export UI | `src/apps/business-os/shared/react-settings.js::exportSupportDiagnosticsArtifact`, `src/apps/business-os/shared/react-settings.js::moduleSupportDiagnosticsHtml`, `src/apps/business-os/shared/react-settings.test.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsRolesPermissionsUiSmoke` |
| Native role and permission evaluator | `src/core/business_os/policy.rs::BusinessOsPermission`, `src/core/business_os/policy.rs::evaluate` |
| Native permission grant table | `src/core/business_os/store.rs` schema for `business_permission_grants` |
| Native command gates for source/release/rollback/install/uninstall | `src/core/business_os/store.rs::accept_rxdb_business_command` |
| MCP read/write policy gates | `src/core/business_os/mcp_channel.rs::business_os_mcp_policy_decision`, `src/core/business_os/mcp_channel.rs::business_os_mcp_module_visibility_decision`, `src/core/business_os/mcp_channel.rs::module_value_visible_to_mcp_actor` |
| Browser DB facade handed to modules | `src/apps/business-os/app.js::createModuleContext`, `src/apps/business-os/app.js::createLiveDbFacade` |
| Permission-aware dynamic-app browser DB guard | `src/apps/business-os/app.js::createLiveDbFacade`, `src/apps/business-os/app.js::createDynamicAppDataGuard` |
| Dynamic app browser smoke matrix | `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js` |
| Production smoke evidence registry | `src/core/rxdb/tools/business_os_production_smoke_registry.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js::evidenceRequirementsForMode` |
| App Store release browser smoke | `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppReleaseUiSmoke`, `src/core/rxdb/tools/browser_rust_smoke.js::prepareBusinessOsReleaseModuleFixture`, `src/core/rxdb/tools/browser_rust_smoke.js::seedBusinessOsReleaseNativeSetup` |
| App versioning rule in app-creator prompt contract | `src/core/business_os/store.rs::business_os_app_command_target_prompt_block` |
| Native lifecycle catalog projection/backfill | `src/core/business_os/store.rs` |
| Native module catalog sync repair/upsert fallback | `src/core/business_os/rxdb_peer.rs` |
| Business OS HTTP data-route gate | `src/core/business_os/server.rs::handle_request`, `src/core/business_os/server.rs::is_business_os_control_plane_path` |
| No legacy HTTP module source/release/rollback guard | `src/apps/business-os/scripts/assert-rxdb-only.mjs::assertBusinessOsServerHttpDataApisAreGated` |
| Release write-order and stale-version consistency guard | `src/core/business_os/store.rs::ensure_module_version_ref_exists`, `src/core/business_os/store.rs::record_module_release`, `src/core/business_os/store.rs::rollback_module_release`, `src/core/business_os/store.rs::sync_module_release_records` |
| Native release/rollback audit events | `src/core/business_os/store.rs::insert_business_event`, `src/core/business_os/store.rs::accept_rxdb_business_command`, `src/core/business_os/store.rs::module_release_and_rollback_write_business_event_audit`, `src/core/business_os/store.rs::module_release_failed_validation_writes_business_event_audit`, `src/core/business_os/store.rs::module_release_rollback_failed_outcome_writes_business_event_audit` |
| Settings Activity release/rollback audit UI | `src/apps/business-os/shared/react-settings.js::activityTitle`, `src/apps/business-os/shared/react-settings.js::activityDetail`, `src/apps/business-os/shared/react-settings.test.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppReleaseUiSmoke` |
| Native release state/data-access lifecycle projection | `src/core/business_os/store.rs::projected_module_lifecycle`, `src/core/business_os/store.rs::module_release_lifecycle_summary`, `src/core/business_os/store.rs::release_review_data_access_projection`, `src/core/business_os/store.rs::module_catalog_for_rxdb`, `src/core/business_os/store.rs::sync_module_release_records` |
| Runtime-installed release/rollback manifest root consistency | `src/core/business_os/store.rs::resolve_business_os_installed_app_root`, `src/core/business_os/store.rs::module_manifest_path`, `src/core/business_os/store.rs::app_root_for_module_manifest`, `src/core/business_os/store.rs::record_module_release`, `src/core/business_os/store.rs::rollback_module_release` |
| Lifecycle release projection repair | `src/core/business_os/store.rs::repair_module_lifecycle_projections`, `src/core/business_os/store.rs::module_lifecycle_projection_repair_resyncs_releases_and_catalog`, `src/core/business_os/rxdb_peer.rs::accept_pending_business_command` |

Current behavior:

- Packaged apps are visible unless hidden by internal/allowlist rules.
- Runtime-installed apps with SemVer major `>= 1` are team-visible in the
  Shell and App Store by default.
- Runtime-installed apps with SemVer major `0` are visible in normal app
  discovery only to assigned App-Verantwortliche:r or actors with an exact
  `apps.view` grant for that module. Exact `apps.modify`, `apps.source.view`,
  `apps.release` and `apps.rollback` grants no longer make private drafts
  visible. Global Owner/Admin authority does not make private drafts appear as
  normal team apps.
- Runtime-installed apps with explicit restricted lifecycle state are hidden
  from Team and visible to assigned App-Verantwortliche:r or exact `apps.view`
  grants until a durable named audience model is implemented.
- Missing or invalid SemVer on a runtime-installed app is treated as not
  public and warns through the lifecycle projection.
- Native module catalog projection now writes lifecycle metadata into each
  module plus `business_module_catalog.governance.lifecycle`. Projected
  `current_semver=null` is authoritative and prevents stale manifest versions
  from making invalid installed apps Team-visible.
- Native `ctox.business_os.why` diagnostics explain app visibility/open/edit/
  source/release/rollback and per-data-area read/write decisions for a selected
  actor and module from the same catalog projection plus central policy engine.
  The visibility explanation intentionally follows the browser lifecycle rule:
  private `0.x` apps remain hidden from normal team discovery even when an
  Owner/Admin can still manage them through policy-authorized operations.
  This diagnostics command persists only sanitized module id and actor context
  for its command projection.
- Settings module management now exposes a `Warum?` action per manageable app.
  The action dispatches `ctox.business_os.why` through the existing
  `business_commands` path and renders the returned native diagnostics through
  the shared diagnostics component. Static tests reject raw policy keys,
  nested decision payloads, reason codes and prompt/token/selection leakage;
  the live `business-os-roles-permissions-ui` Browser/Rust smoke proves the
  real Settings click, native projection wait, required diagnostics rows and
  redaction budget with browser errors/404/request failures 0.
- Native `ctox.business_os.support.export_diagnostics` exports a
  support-safe diagnostics artifact over the same `business_commands` path.
  The artifact has schema `ctox.business_os.support_diagnostics.v1`, includes
  a redaction manifest, actor/scope, summarized Activity rows and an optional
  sanitized Why summary, and deliberately omits raw event payloads, prompt
  bodies, message bodies, record payloads, tokens and secrets. Rust tests prove
  sensitive markers do not leak into the result or stored command projection.
- Native `ctox.business_os.audit.retention` exports expired `business_events`
  as a support-safe `ctox.business_os.audit_retention_export.v1` artifact under
  `runtime/business-os/audit-exports` before optional prune. The command is
  gated by `users.manage`, keeps only sanitized retention/prune command
  projection, and denies spoofed Teammitglied attempts without producing an
  export.
- Native `ctox.module.repair_lifecycle_projection` supports `dry_run` for
  release/catalog projection drift, returns the planned actions, performs no
  `business_records`/RxDB/catalog mutation during dry-run, and persists only a
  sanitized recovery command projection.
- Browser UI affordances for `App aendern` and `Source oeffnen` use the shared
  permission projection.
- Shell tabs, launcher/start-menu items, module appbar and App Store
  cards/details show lifecycle state and version for runtime-installed apps.
  Launcher/start-menu lifecycle badges are real buttons and open the
  permission-aware lifecycle drawer without launching the app.
- Runtime-installed modules and the packaged/starter user-module batch
  (`conversations`, `cv-print-builder`, `documents`, `spreadsheets`,
  `support`) now
  receive a guarded `ctx.db` facade through the normal `createModuleContext(mod)`
  -> `createLiveDbFacade(mod)` Shell context path.
  Browser/Rust dynamic-app smoke verifies collection access, collection
  properties, cached handles and `raw` denial, plus cached-handle read success
  after an explicit `data.read` grant. The same Browser/Rust path now covers a
  persisted runtime-installed fixture mounted through real `openModule` after
  reload; the packaged batch proves read/raw/context denial before `data.read`,
  `ctx.permissions` deny/allow parity, Support's real Shell locked state
  without data grants, read success after an exact collection grant and write
  denial without `data.write`. Remaining packaged/core module migration or
  tested exceptions remain in Phase 13.
- Runtime-installed/generated app code is explicitly treated as same-origin
  trusted Business OS app code, not sandboxed iframe code. The Shell exposes
  `ctx.runtimeCapabilities` with the `business-os-runtime-capabilities-v1`
  contract: local module-template fetch only, relative static imports only,
  guarded DB handles, no authoritative browser storage, no direct Shell-global
  mutation, no Worker/service-worker path and no direct CTOX control commands
  from generated apps. External effects for generated apps stay routed through
  `ctx.commandBus` with `business_os.chat.task`; server-side policy remains the
  authority for anything that mutates CTOX state.
- The installed App Creator validator rejects forbidden runtime capability
  bypasses for generated apps: arbitrary network fetch, dynamic import,
  local/session storage, Shell global state, cached `ctx.db` handles, Worker
  launch, direct navigation, dynamic evaluators and direct CTOX control
  commands. Browser/Rust `business-os-dynamic-apps-ui` proves the persisted
  runtime-installed fixture sees the same contract after reload through the real
  `openModule` path.
- Browser UI preference storage is scoped by workspace and actor where relevant.
  Shell taskbar pins, module layout, account preferences, Shell column/module
  resizer widths and App Store pane width no longer share one global browser
  key across users/workspaces; modules receive `ctx.storageScope` with the
  `business-os-storage-scope-v1` contract. Pairing config remains a
  workspace-scoped startup hint with legacy fallback and cleanup only. Browser
  storage remains non-authoritative: app visibility, audience, release state
  and data grants still come from native/RxDB governance and policy, not
  `localStorage`.
- Native commands and MCP tool calls are policy-gated for the covered
  operations. MCP module listing now exposes apps through lifecycle public
  state or exact `apps.view`, not `data.read`; module detail/proposal paths
  require app visibility before `data.read`, module links require app
  visibility only, and execution requires app visibility before `data.write`.
- The Shell global right-click CTOX menu now renders a compact `CTOX Zugriff`
  scope panel before submit. It shows the acting user, selected app/version/
  lifecycle, selected record, data-access summary and external-action state,
  and submits the same object as `client_context.visible_scope`. The command
  bus also writes canonical module/app/action/mode/target/record/scope aliases
  into persisted command context. Coding Agents include provider, workspace and
  session scope in `client_context`, and scheduled Business Chat dispatches
  preserve existing `chat.contextMeta.client_context`. Business Chat renders the
  preserved visible scope rows in the chat window when a chat starts from
  scoped context. App Store context chat renders/builds selected-app visible
  scope from lifecycle/data projection and submits it in `client_context`.
  Browser/Rust `business-os-agent-scope-ui` proves the global right-click,
  App Store context-chat and Business Chat paths: visible scope rows match
  submitted `client_context`, the App Store panel matches its submitted
  selected-app scope, Business Chat renders the same submitted visible scope,
  private hidden apps stay denied, data read only opens after an exact grant,
  writes stay denied without `data.write`, and persisted command audit keeps
  visible scope.
- Native policy audit events now store redacted, scope-only
  `client_context` with app/module/record fields and `visible_scope`; MCP
  activity events store `business_scope` with tool/module/action/collection/
  record identifiers. Prompt text, selected text, MCP title/objective/query and
  payload content are deliberately excluded from those audit metadata fields.
- Native Team release review now validates reviewed read/write collections
  against the manifest and reconciles every reviewed data area with explicit
  Team `data.read`/`data.write` grants or a declared locked-state behavior.
  The review remains evidence-only and does not create grants.
- Legacy HTTP module source/release/rollback handlers are removed from
  `server.rs`; source/release/rollback must stay on RxDB/WebRTC
  `business_commands`, and the RxDB-only guard blocks reintroducing those
  direct HTTP route/helper markers.
- Native release rejects stale `source_version_id` and `rollback_version_id`
  before writing `module.json`; release row plus manual source-version summary
  writes run in one SQLite transaction; release and release rollback restore
  `module.json` when injected DB updates fail.
- Native release/rollback command outcomes write queryable `business_events`
  for successful release, successful rollback, failed release validation and
  failed rollback outcomes. Activity-list command responses include those event
  types with business-facing summaries instead of raw manifest/source payloads.
- Native release projection now writes `release_status`, `release_state`,
  `rollback_target` and `data_access` summaries into lifecycle governance.
  `data_access` separates granted and locked collection ids and keeps review
  evidence-only with `grants_implied=false`.
- Shared browser lifecycle logic now reads the native release projection and
  renders release, rollback and granted/locked data-area summaries in App Store
  cards/details, the Shell lifecycle drawer and Settings module-management
  rows. Settings read-only fallback is also Browser/Rust-proven; broader
  App Store release reload proof is now Browser/Rust-proven by
  `business-os-app-release-ui`.
- Settings release/rollback controls are downgraded to read-only diagnostics:
  there is no active Settings `ctox.module.release` or `ctox.module.rollback`
  dispatch path. The live Settings drawer has Browser/Rust evidence for
  disabled diagnostics. Settings Activity also has Browser/Rust evidence for
  release and rollback lifecycle audit rows with redacted, business-facing
  labels.
- Runtime-installed release/rollback resolves the actual installed module
  manifest path before source snapshot/version writes, so installed apps do not
  accidentally snapshot against the fallback app root.
- Release command replay is idempotent for accepted `ctox.module.release`
  commands: a replay returns the stored accepted outcome and does not add a
  duplicate release row, manual source-version snapshot or release audit event.
- `ctox.module.repair_lifecycle_projection` repairs release lifecycle
  projections through the native command path: canonical
  `business_records`, RxDB `business_module_releases` and the
  `business_module_catalog` are resynchronised from native release state and the
  real runtime-installed app root.
- The App Store publish path now has live Browser/Rust evidence: a private
  runtime-installed `0.8.0` app is visible to the App-Verantwortliche but not to
  Team, is published as `1.0.0` through the real App Store release dialog and
  native `business_commands`, becomes Team-visible only after native catalog
  projection, shows version/data-review facts after reload, ignores a
  localStorage attempt to widen/narrow authority, and rolls back through the
  version dialog.

Remaining gaps:

- The App Store release/data-access review path is implemented and has
  Browser/Rust proof for publish/reload/rollback plus Settings Activity
  release/rollback audit evidence. Remaining release-gate integration is
  tracked in Phase 16, not as a Phase 10 implementation blocker. The legacy
  HTTP module source/release/rollback server handlers are closed by Phase 10C,
  and the active publish path stays on RxDB/WebRTC `business_commands`.
- Agent-scope global right-click, App Store context-chat and Business Chat
  rendered-scope paths have Browser/Rust proof; Settings now shows active
  agent/app/data Sonderfreigaben as a read-only Owner/Admin boundary. Native/MCP
  audit metadata parity is locally verified.
- Packaged/core modules keep the legacy live DB facade for compatibility; the
  migration or explicit tested exception set remains the open Phase-13
  production blocker.
- A canonical app-line `creator_user_id` is not normalized yet. The current
  production rule uses the stable governance contract:
  `business_module_acl` App-Verantwortliche plus exact `apps.view` grants for
  preview visibility.
- Legacy runtime-app manifests that still carry `lifecycle.preview_user_ids`
  are migrated idempotently into native module-scoped `apps.view` grants during
  catalog projection. Projected `preview_user_ids` now come from active
  `apps.view` grants, not from browser-only display hints.
- Direct Shell opens for hidden private, preview or restricted apps are blocked
  before module import/mount and redirected to a visible fallback with a
  business-facing locked reason.

## Product Model

Apps have four independent but connected layers:

1. Visibility: can the actor discover the app in the Shell/App Store?
2. Execution: can the actor open and use the app UI?
3. Data access: which collections, records and actions can the app expose for
   this actor?
4. App governance: who can inspect source, modify, release, rollback, assign
   owners or change visibility?

Versioning drives the default visibility layer:

| Version / state | Product meaning | Default audience | Typical UI label |
| --- | --- | --- | --- |
| `0.0.x` | Private iteration, UI or bug fix | App builder / app maintainers only | Privat |
| `0.x.y` where `x > 0` | Private or limited preview before team release | App builder / app maintainers, optional testers | Vorschau |
| `1.0.0+` | Team release | Team by default, unless app policy narrows it | Team |
| `2.0.0+` | New app line or major product shift | Team after release, but shown as separate lineage when needed | Neue Linie |

The current source already treats major `0` as not public and major `>= 1` as
public for runtime-installed apps. The product model should make that state
visible, editable and testable.

## Roles And Actors

Business-facing roles:

| Role | Product label | App lifecycle rights |
| --- | --- | --- |
| `chef` / `owner` | Owner | Full workspace, visibility, data, release and recovery authority |
| `admin` | Admin | Manage users, install/uninstall apps, assign App-Verantwortliche, operate released app catalog |
| `founder` | App-Verantwortliche:r | Build, source-view, modify, release/rollback and data-access own assigned apps |
| `user` / `team` | Teammitglied | See and use team-released apps; no app governance by default |
| MCP/service actor | KI/Agent | No implicit admin rights; acts through user identity or exact grants |

The phrase "Entwickler/Ersteller" should map to an explicit product concept:

- Current implementation: "App-Verantwortliche:r" means the active assignment
  in `business_module_acl`; exact `apps.view` grants add named preview viewers,
  while exact `apps.modify` grants add named builders without changing their
  global role.
- Server-side responsibility safety is now part of that contract: private
  runtime apps cannot lose their last active App-Verantwortliche:r through
  Founder assignment removal or user deactivation unless the acting Owner/Admin
  explicitly accepts recovery responsibility. Deactivated users no longer count
  as active responsible users in lifecycle or permission projections.
- Future metadata refinement: track `created_by`/`creator_user_id` on the app
  line for audit, ownership transfer and creator-specific filters.

Recommendation for CTOX Business OS: use "App-Verantwortliche:r" for who sees
and changes private `0.x` apps, because teams need continuity when the original
creator leaves. Preserve `created_by` as audit metadata, not as the only access
key.

## UI/UX Concept

The app icon must carry app lifecycle state without turning the Shell into an
admin table.

### App Icon

Every app icon should expose:

- App mark.
- App name.
- Compact version text, for example `v0.3.2` or `v1.1.0`.
- One lifecycle badge:
  - `Privat` / lock glyph for `0.0.x` and no tester audience.
  - `Vorschau` / small eye glyph for `0.x.y` with testers or explicit grants.
  - `Team` / group glyph for `>= 1.0.0`.
  - `Eingeschraenkt` / shield glyph when the app is released but narrowed by
    allowlist or explicit policy.
- Optional warning dot when version is invalid, release metadata is missing or
  required data-access policy is incomplete.

The badge is clickable for actors who can see the app. Actors with app
management rights see a `Verwalten erlaubt` drawer state and shortcuts into
the App Store control plane. Everyone else sees `Nur Ansicht`, the full
version/visibility reason and a details-only App Store action.

### Badge Click Interaction

Clicking the lifecycle badge opens a small app visibility popover or right
drawer, not a modal.

For App-Verantwortliche/Owner/Admin:

- Current version and lifecycle state.
- Current audience: "Nur App-Verantwortliche", "Vorschaugruppe", "Team",
  "Eingeschraenkt".
- Primary next action:
  - `Als Team-Version veroeffentlichen` when version can become `1.0.0`.
  - `Vorschaugruppe bearbeiten` for `0.x` preview.
  - `Sichtbarkeit einschraenken` for released apps.
- Data-access summary: read/write collections affected by this app.
- App governance shortcuts: App-Verantwortliche, Source, Versionen, Activity.

For Teammitglieder:

- Read-only lifecycle state.
- Clear unavailable reason when an app is private or data is not accessible.
- Optional "Zugriff anfragen" as a future workflow, but not as a fake button
  until the approval path exists.

### App Store Detail

The App Store should become the heavier management surface. The detail pane for
an app should have four compact sections:

1. Version and release state.
2. Visibility audience.
3. Data access policy.
4. App governance and activity.

This avoids hiding important governance behind only the icon badge. The icon
badge is the shortcut; App Store is the full control plane.

## Permission Semantics

Use separate permissions for separate product questions.

| Question | Permission / rule |
| --- | --- |
| Can see app in Shell/App Store | App lifecycle visibility plus instance allowlist plus exact `apps.view` for non-public audiences |
| Can open released app | Team-visible app unless allowlist/policy narrows it |
| Can see private `0.x` app | App-Verantwortliche:r or exact module `apps.view`; global Owner/Admin authority and exact modify/source/release grants alone do not make private drafts appear in normal discovery |
| Can change app | `apps.modify` on module |
| Can see source | `apps.source.view` on module |
| Can publish app | `apps.release` on module |
| Can rollback app | `apps.rollback` on module |
| Can manage app owners | `apps.assign_owner` on module |
| Can read module data through MCP/native API | `data.read` on module, collection or exact record |
| Can write module data through MCP/native API | `data.write` on module, collection or exact command path |
| Can approve external effects | `external.approve` on module or exact approval |

Do not treat "app is visible" as "all app data is readable". A released app can
be visible while showing permission-limited empty states for restricted
collections.

## Browser Data Access Model

Hard production isolation for runtime-installed apps will use a
permission-aware browser DB facade in the real Shell mount path.

Implemented core behavior in the dynamic-app and Phase-13B Shell-context
slices:

- `ctx.db.collection(name)` checks the current actor, active module and
  collection-to-module mapping before returning a collection handle.
- `ctx.db.<collection>` uses the same check.
- `ctx.db.raw` is replaced with a guarded proxy for runtime-installed modules.
- The normal dynamic module context passes the active module into the live DB
  facade via `createModuleContext(mod)` -> `createLiveDbFacade(mod)`.
- Denied reads return a typed permission error that modules can render as a
  locked state.
- Denied writes fail before mutation with a typed permission error and command
  audit entry where applicable.
- App modules receive a compact `ctx.permissions` helper so UI can render
  locked states without probing data.

Production blockers:

- Phase 13B now has Browser/Rust proof for both the exported Shell context
  helper and a persisted runtime-installed app mounted through real
  `openModule(mod)` after reload.
- Phase 13C now has packaged/starter user-module proof for `coding-agents`,
  `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`, `documents`,
  `invoices`, `iot`, `matching`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support`: the Shell
  marks them as guarded packaged modules, `ctx.runtimeCapabilities` reports
  guarded DB/raw/property/cached handles, and the Dynamic Apps Browser/Rust
  smoke requires collection/property/raw/context denial before `data.read`,
  `ctx.permissions` deny/allow parity, a real Support Shell locked state
  without data grants, read success after an exact collection grant and write
  denial without `data.write`. `coding-agents`, `calendar`, `buchhaltung`, `customers`,
  `invoices`, `iot`, `matching`, `notes`, `outbound`, `research` and `shiftflow` are covered against module-owned
  collections after targeted schema registration.
- `customers` now degrades optional linked cross-app projections to an empty
  linked-data state on permission denial instead of aborting the core CRM load.
  `invoices` now exposes only a redacted `window.__ctoxInvoicesModule.inspect()`
  snapshot instead of leaking module `STATE`, `ctx` or `ctx.db` through the
  browser debug bridge. `iot` now resolves collections through the guarded
  facade instead of preferring `ctx.db.raw`. `notes` now resolves through the
  guarded facade instead of `ctx.db.raw`/`notes_records`, and LocalStorage is
  no longer used as an authoritative note-data fallback. `calendar` now
  resolves all Calendar collections through the guarded facade instead of
  `ctx.db.raw` and gates default seed writes behind collection write
  permission. `outbound` now resolves Active Outreach through the guarded
  facade, gates automatic default/import-repair writes behind collection write
  permission, and treats `ctox_queue_tasks` as optional read-permission-aware
  operational status outside the Outbound module grant. `research` now resolves
  data through `ctx.db.collection(name)`, gates task/run writes, keeps
  command/queue projections optional, and declares/gates Fachbericht
  `documents`/`document_versions`/`document_blob_chunks` as explicit
  manifest/schema collections. `shiftflow` now removes its global/DOM cached
  DB handles, skips startup seed writes without exact planning collection
  write grants and subscribes through guarded helpers; runtime planning actions
  still use explicit guarded collection-property access. `buchhaltung` now
  resolves accounting collections through the guarded facade, removes raw DB
  and global state exposure, gates chart/demo seed writes behind exact
  accounting collection write grants, reconciles `accounting_number_series`
  metadata and removes the UI-E2E localStorage asset data fallback. `matching`
  now receives the Shell Business OS context via `setBusinessOsDatabaseContext(ctx)`,
  resolves Matching collections through `ctx.db.collection(name)`, gates writes
  through `ctx.permissions.canWriteCollection`, and no longer injects `ctx.db.raw`
  or opens a standalone CTOX Sync Engine fallback.
- Browser, CTOX, Knowledge, Reports and Tickets no longer use direct
  `ctx.db.raw` access. Their system collections now resolve through
  `ctx.db.collection(name)`. Creator runtime and generated app templates no
  longer use collection-property or `ctx.db.collections` fallback access. App
  Store, Browser, Creator, CTOX, Desktop, Knowledge, Reports and Tickets now
  have exact scoped exception allowlists validated against
  `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS`.
- The former unscoped Desktop, Chat, Report and Settings/internal facades now
  use `createScopedSystemDbFacade(scopeName, collectionNames)` with explicit
  collection allowlists and are guarded by
  `src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`. The
  current inventory has 0 unscoped facade entries; Phase 13 is complete
  locally for packaged/core DB isolation.
- Static guards now require inventory updates for new raw/property/proxy/cache
  DB-access shapes in packaged modules. Runtime guards must still cover newly
  installed generated app code so
  network, import, storage and external-effect capabilities cannot bypass the
  product contract.

Follow-up: Phase 13 has no remaining DB-isolation blocker. Future tightening
can still migrate selected system surfaces from tested scoped exceptions to
first-class data grants once those operational reads/writes become normal
tenant data flows instead of Shell/control-plane duties.

Facade validation and implementation on 2026-06-18 split and closed the
formerly unscoped-facade work into business-facing surfaces:

- Settings is an admin/system surface and should receive only the module
  catalog and command collections needed for module management, with
  role/permission checks.
- Desktop app windows need app-specific facades, not one generic desktop
  facade: Browser runtime, Code-Editor source work, Creator app-building,
  Explorer file management and File-Viewer materialization each require
  different collection allowlists.
- Business Chat needs a narrow chat system facade for chat, command, queue and
  attachment file collections, plus actor/owner row filtering.
- Business Reporter needs a reporter system facade for report, bug-report and
  command projections, with active-module metadata but without broad
  active-module data access.

Implemented result: Settings, Business Chat, Business Reporter and each
Desktop app now receive scoped collection allowlists through
`createScopedSystemDbFacade`; Browser/Rust `business-os-agent-scope-ui`,
`business-os-roles-permissions-ui`, `workspace-large-file-viewer-rust-to-browser`
and `business-os-ui-regression` pass for the representative paths. The
UI-regression pass is functionally green but keeps the known Notes
contenteditable/flex browser warning.

## App Lifecycle Flows

### Build A Private App

1. App is created with `version=0.1.0`, `install_scope=installed`, creator set
   as App-Verantwortliche:r.
2. Icon appears only for the App-Verantwortliche:r and exact `apps.view`
   preview grants; exact modify/source/release grants and global Owner/Admin
   authority alone do not make it appear in normal team discovery.
3. Icon badge shows `Privat` and `v0.1.0`.
4. App Store detail shows required collections and missing release checks.

### Share A Preview

1. App-Verantwortliche clicks lifecycle badge.
2. Selects "Vorschaugruppe".
3. Adds users or role-scoped preview grants.
4. Icon badge changes to `Vorschau`.
5. Preview users can see/open the app but only access data allowed by the data
   policy.

### Release To Team

1. App-Verantwortliche opens lifecycle badge or App Store detail.
2. Chooses "Als Team-Version veroeffentlichen".
3. Flow requires valid SemVer `>= 1.0.0`, source/version snapshot and data
   access review.
4. On success, app appears for Team by default.
5. Icon badge shows `Team`, version shows `v1.0.0`.

### Restrict A Released App

1. Owner/Admin/App-Verantwortliche opens App Store detail.
2. Changes audience from Team to explicit group or role.
3. Icon badge changes to `Eingeschraenkt`.
4. Team members outside the audience do not see the app, or see a locked
   request state only if product later wants a request workflow.

## AI / Agent Access

KI/Agenten should not get a parallel permission model.

- An agent acts as a Business OS actor with role `user` unless persisted as a
  Business OS user or given exact grants.
- App visibility for agents follows the same app lifecycle and grant model.
- Data access for agents follows `data.read`/`data.write` with module,
  collection, record and command scopes.
- MCP backend paths already enforce that app visibility and data access are
  independent decisions: a data grant alone does not reveal a hidden app, and
  app visibility alone does not unlock data reads or writes.
- External effects remain separately gated by `external.approve`.
- Agent UI should display "handelt als <user/app actor>" and show which app
  and data scopes are active before an irreversible action.

## Implementation Plan

### Phase 8A: Source-Validated Current Rule Tests

Status: Complete for the core slice.

- Add targeted JS tests for `canSeeModuleForAppVersion` in Shell and App Store.
- Cover installed `0.1.0`, installed `1.0.0`, malformed version and packaged
  app behavior.
- Cover App-Verantwortliche, Owner/Admin, Team, exact `apps.view` grant and
  exact `apps.modify` as a negative visibility case.

### Phase 8B: App Lifecycle Projection

Status: Complete for native catalog projection/backfill. The App Store
release/data-review Browser/Rust flow is now covered by Phase 10F/10H;
audience management, Activity/audit browser evidence and CI release gates
remain future work.

- Add first-class lifecycle fields to the existing module catalog projection,
  not a new data plane:
  - `lifecycle.visibility_state`
  - `lifecycle.version`
  - `lifecycle.audience`
  - `lifecycle.preview_grant_ids`
  - `lifecycle.preview_user_ids` from active `apps.view` grants
  - `lifecycle.release_required_checks`
  - `lifecycle.creator_user_id` if strict creator semantics are needed.
- Keep the RxDB/WebRTC-only path.

### Phase 8C: Shell Icon Badges

Status: Complete for Shell tabs, launcher/start-menu items, module appbar and
App Store cards/details. Phase 14 keeps broader visual/narrow-viewport label
QA, but launcher/start-menu lifecycle/version badges are no longer a Phase 11
gap.

- Render version and lifecycle badge on Shell tabs, module appbar, App Store
  cards/details and launcher/start-menu app-choice items.
- Badge click opens visibility popover/drawer for authorized actors.
- Read-only tooltip for unauthorized actors.
- Add German/English labels and long-label tests.

### Phase 8D: App Store Governance Panel

Status: Complete for App Store lifecycle/version details and the core
release/data-review publish flow. Preview/restricted audience management and
broader production visual/audit gates remain later phases.

- Add Version, Sichtbarkeit, Datenzugriff and Verantwortliche sections to app
  detail.
- Wire "Team-Version veroeffentlichen" to the existing release command path.
- Require data-access review before moving to `1.0.0`.

### Phase 8E: Permission-Aware Browser DB Facade

Status: Complete for runtime-installed/dynamic apps.

- Replace `ctx.db.raw` access in modules with guarded collection access.
- Enforce `data.read`/`data.write` per active module, collection and actor.
- Return typed permission errors and render locked states.
- Static repo-module conformance forbids new direct raw DB access in packaged
  modules; external runtime module linting remains future hardening.

### Phase 8F: Agent/App Scope UI

Status: Complete for the covered Phase-12 local slice. MCP backend
app-visibility-before-data is implemented in Phase 12A. The first browser
slice is implemented for global right-click:
visible actor/app/selection/data/external scope is shown before submit, the
same object is submitted as `client_context.visible_scope`, the command bus
normalizes persisted scope aliases, and Coding Agents include provider/
workspace/session external context. Scheduled Business Chat commands preserve
existing `contextMeta.client_context`, and Business Chat renders preserved
visible scope rows in the chat window. App Store context chat renders/builds
selected-app visible scope and submits it in `client_context`. Browser/Rust
`business-os-agent-scope-ui` now proves the global right-click, App Store
context-chat and Business Chat paths: visible panel rows match submitted
`client_context.visible_scope`, the App Store context menu matches its submitted
selected-app scope, Business Chat renders that same submitted visible scope,
hidden private app open is denied, data read is denied before and allowed after
an exact grant, write remains denied without `data.write`, and persisted
command audit keeps visible scope. The same Browser/Rust smoke proves the
Settings Owner/Admin grant-boundary panel renders active Sonderfreigaben without
adding UI-only grant mutation. Native policy events persist redacted
scope-only `client_context`, MCP events persist `business_scope`, and tests
prove free-form prompts, selected text and MCP payloads are not copied into
audit metadata.

- Surface active actor and app/data scopes in AI-assisted app interactions.
- Ensure MCP/service actors share the same lifecycle and data-scope rules.

### Phase 8G: Production Gates

Status: Complete for the core slice.

- Add live Browser/Rust smoke:
  - Team cannot see private `0.x` app.
  - App-Verantwortliche can see private `0.x` app.
  - Team sees `1.0.0` app.
  - Released app with restricted audience is hidden/locked as designed.
  - Data-denied runtime app cannot read/write collections through `ctx.db` or
    `ctx.db.raw`; app-specific locked-state rendering remains module work.
- Rerun RxDB guard suite, JS permission tests, full UI matrix and
  `cargo test --bin ctox business_os`.

### Phase 9: Native Lifecycle Projection

Status: Complete.

- Project lifecycle metadata through the existing native module catalog and
  `governance.lifecycle`, without a new RxDB collection.
- Backfill packaged modules as `packaged/system`, invalid installed apps as
  private warning state, `0.x` installed apps as private or preview with exact
  grants, `1.0.0+` installed apps as Team and restricted apps as restricted.
- Keep creator/release audit metadata separate from App-Verantwortliche
  responsibility and exact grants.
- Keep orphan prevention server-authoritative: UI shortcuts may explain or
  preselect recovery, but `ctox.module.assign_founder` and user-management
  commands remain the enforcement point.
- Make projected `current_semver` authoritative in browser lifecycle code.
- Preserve WebRTC/RxDB-only guardrails; no HTTP fallback and no dist patch.

## Core Slice Verification Evidence

Passed locally on 2026-06-17:

- `node src/apps/business-os/shared/app-lifecycle.test.mjs`
- `node src/apps/business-os/modules/app-store/app-store.test.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `CARGO_TARGET_DIR=runtime/build/core-rxdb-integration-target cargo build --bin ctox`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18989 SIGNALING_PORT=28989 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `rustfmt --edition 2021 src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs --check`
- `node --test src/apps/business-os/shared/app-lifecycle.test.mjs`
- `cargo test --bin ctox business_app_semver_major_matches_browser_plain_semver_contract`
- `cargo test --bin ctox module_catalog`
- `cargo test --bin ctox business_os`
- `cargo build --bin ctox`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright CTOX_BIN=runtime/build/cargo-target/debug/ctox SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html /Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node src/core/rxdb/tools/browser_rust_smoke.js`

The dynamic-app smoke asserts hard Evidence for private `0.x`, Team-default
`1.0.0`, restricted released apps, invalid versions, lifecycle badge/drawer
rendering, denied `ctx.db` reads, denied `ctx.db.raw` reads, collection-level
read grants, denied writes without `data.write` and reload stability for the
projected native lifecycle state.

Additional Phase 13A inventory evidence passed locally on 2026-06-17:

- `docs/business-os-db-isolation-inventory.json` classifies 24 packaged/core
  modules, 5 desktop apps and 4 unscoped Shell facades with owner, review date,
  migration/exception status and current DB-access shape.
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`

Additional Phase 13D static drift evidence passed locally on 2026-06-17 and
hardened on 2026-06-18 after source-read validation:

- `src/apps/business-os/scripts/assert-module-conformance.mjs` invokes the
  DB-isolation inventory guard, so the standard module conformance gate now
  fails when a module or unscoped facade introduces raw DB access, collection
  property access, `ctx.db.collections` proxy access or cached/exported DB
  handles without an explicit inventory update. The inventory guard also
  requires every packaged/core module to declare all DB-access flags as
  booleans, including explicit `false` entries.
- The 2026-06-18 hardening closes optional-chaining, dynamic-property and local
  alias blind spots: `ctx?.db?.raw`, `ctx?.db?.[name]`,
  `ctx?.db?.collections` and `const db = state.ctx?.db; db?.raw` /
  `db?.collections` are detected. The committed inventory was updated to match
  current source truth for `coding-agents`, `creator`, `ctox`, `customers`,
  `cv-print-builder`, `iot`, `notes` and `support`. This keeps Phase 13C honest
  but does not close packaged/core migration or tested exceptions.
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases after hardening
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node --check src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`

Additional Phase 13C packaged-module migration evidence passed locally on
2026-06-18:

- `conversations`, `cv-print-builder`, `documents` and `spreadsheets` are now
  the guarded packaged/starter user-module batch. The Shell guarded DB facade
  is active for runtime-installed modules plus this batch; remaining packaged/
  core modules still require migration or tested exceptions.
- `docs/business-os-db-isolation-inventory.json` marks only this batch as
  `guarded-facade-migrated` and keeps the remaining user/system modules open.
- `node --check src/apps/business-os/app.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60651 SIGNALING_PORT=60652 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_packaged_guard_modules=conversations,cv-print-builder,documents,spreadsheets`,
  `business_os_dynamic_packaged_guard_count=4`,
  `business_os_dynamic_packaged_guard_batch_coverage=1`,
  `business_os_dynamic_packaged_guard_all_capability_contracts=1`,
  `business_os_dynamic_packaged_guard_all_read_denied=1`,
  `business_os_dynamic_packaged_guard_all_property_denied=1`,
  `business_os_dynamic_packaged_guard_all_raw_denied=1`,
  `business_os_dynamic_packaged_guard_all_context_denied=1`,
  `business_os_dynamic_packaged_guard_all_read_grants_allowed=1`,
  `business_os_dynamic_packaged_guard_all_writes_denied_without_write=1`,
  browser warnings/errors/404/request failures 0 and
  `startup_smoke_hook_reload_count=0`.

Additional Phase 13C Support guarded-module evidence passed locally on
2026-06-18:

- `support` is now added to the guarded packaged user-module batch. The Shell
  exposes `ctx.permissions` next to `ctx.db`, renders an explicit Support
  Shell locked state when the app is visible but data grants are missing, and
  logs that expected lock without a browser console error.
- `research` and `shiftflow` remain open by source validation: `research`
  needs explicit grants or UI feature gates for document collection reads, and
  `shiftflow` needs cached global/DOM DB handles removed plus
  permission-aware seed/write behavior.
- `node --check src/apps/business-os/app.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60781 SIGNALING_PORT=60782 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_packaged_guard_modules=conversations,cv-print-builder,documents,spreadsheets,support`,
  `business_os_dynamic_packaged_guard_count=5`,
  `business_os_dynamic_packaged_guard_shell_locked_state=1`,
  `business_os_dynamic_packaged_guard_context_permission_facade=1`,
  `business_os_dynamic_packaged_guard_all_context_permission_facades=1`,
  all packaged guard deny/grant/write keys 1, browser
  warnings/errors/404/request failures 0 and
  `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=82`.

Additional Phase 13C Customers/Coding Agents/Calendar/Invoices/IoT/Notes/Outbound/Research guarded-module evidence passed locally on
2026-06-18:

- `customers`, `coding-agents`, `calendar`, `invoices`, `iot`, `notes` and
  `outbound` were added to the guarded packaged user-module batch in the first
  part of this slice; `research` is now added in the follow-up slice. The
  Dynamic Apps smoke registers missing module-owned schemas per packaged guard
  spec, so Coding Agents is checked against `coding_agent_sessions`, Calendar
  against `calendar_events`, Customers against `customer_accounts`, Invoices
  against `accounting_invoices`, IoT against `iot_widgets`, Notes against
  `notes`, Outbound against `outbound_campaigns` and Research against
  `research_tasks`.
- The CRM core data path stays strict, while optional linked cross-app
  projections degrade to an empty linked-data state on permission denial
  instead of aborting the core CRM load.
- Invoices no longer exposes `STATE`, `ctx` or `ctx.db` through the browser
  debug bridge. `window.__ctoxInvoicesModule` now exposes only `mount` and a
  redacted `inspect()` snapshot.
- IoT no longer prefers `ctx.db.raw`; its collection resolver now uses
  `ctx.db.collection(name)`, `ctx.db.collections[name]` or guarded collection
  property fallback. Runtime mutations continue through the Business OS command
  bus.
- Notes no longer unwraps `ctx.db.raw` or legacy `notes_records`. LocalStorage
  remains a UI/test mirror and layout/app-lock store, but is no longer used as
  an authoritative note-data fallback when DB access is missing or denied.
- Calendar no longer unwraps `ctx.db.raw`; `calendarCollection(name)` resolves
  module collections through `ctx.db.collection(name)`,
  `ctx.db.collections[name]` or guarded collection properties, and default seed
  writes are skipped unless `ctx.permissions.canWriteCollection` allows the
  seeded collections.
- Outbound no longer unwraps `ctx.db.raw` in Active Outreach; automatic
  default-campaign/import-repair writes are skipped unless
  `ctx.permissions.canWriteCollection` allows the affected collections, and
  `ctox_queue_tasks` is treated as optional read-permission-aware operational
  status instead of part of the Outbound module grant. The Shell fallback
  manifest now includes `outbound_skillbooks`/`outbound_letter_templates`.
- Research now resolves module data through `ctx.db.collection(name)`, gates
  task/run writes through `ctx.permissions.canWriteCollection`, keeps
  `business_commands`/`ctox_queue_tasks` as optional operational projections,
  and declares `documents`, `document_versions` and `document_blob_chunks` as
  explicit manifest/schema collections for the Fachbericht viewer. Missing
  document grants lock only the report content path, not the whole dashboard.
- Remaining source-validated cleanup candidates at this checkpoint were
  `shiftflow`, `buchhaltung` and `matching`; `shiftflow` is closed by the
  immediately following slice.
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/customers/index.js`
- `node --check src/apps/business-os/modules/invoices/index.js`
- `node --check src/apps/business-os/modules/iot/index.js`
- `node --check src/apps/business-os/modules/notes/index.js`
- `node --check src/apps/business-os/modules/calendar/index.js`
- `node --check src/apps/business-os/modules/outbound/index.js`
- `node --check src/apps/business-os/modules/outbound/active-outreach.js`
- `node --check src/apps/business-os/modules/research/index.js`
- `node --check src/apps/business-os/modules/research/schema.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`
- `python3 -m json.tool src/apps/business-os/modules/research/module.json >/dev/null`
- `python3 -m json.tool src/apps/business-os/modules/customers/locales/de.json >/dev/null`
- `python3 -m json.tool src/apps/business-os/modules/customers/locales/en.json >/dev/null`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/modules/invoices/tests/invoice-types.test.mjs`
  passed 9/9 checks.
- `node src/apps/business-os/modules/iot/iot.test.mjs` was attempted
  separately, but this workspace does not currently install/resolve `esbuild`
  for that module test.
- `node src/apps/business-os/modules/notes/notes.test.mjs` was attempted
  separately, but this workspace does not currently install/resolve `esbuild`
  for that module test.
- `node src/apps/business-os/modules/calendar/calendar.test.mjs` was attempted
  separately, but this workspace does not currently install/resolve `esbuild`
  for that module test.
- `node src/apps/business-os/modules/outbound/outbound.test.mjs` was attempted
  separately, but this workspace does not currently install/resolve `esbuild`
  for that module test.
- `node src/apps/business-os/modules/research/test.mjs` was attempted
  separately, but this workspace does not currently install/resolve `esbuild`
  for that module test.
- `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/customers/index.js src/apps/business-os/modules/customers/locales/de.json src/apps/business-os/modules/customers/locales/en.json src/apps/business-os/modules/invoices/index.js src/apps/business-os/modules/iot/index.js src/apps/business-os/modules/notes/index.js src/apps/business-os/modules/calendar/index.js src/apps/business-os/modules/outbound/index.js src/apps/business-os/modules/outbound/active-outreach.js src/apps/business-os/modules/research/index.js src/apps/business-os/modules/research/schema.js src/apps/business-os/modules/research/module.json src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md`
- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  completed with existing warnings.
- Accepted Browser/Rust smoke:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61191 SIGNALING_PORT=61192 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_packaged_guard_module=coding-agents`,
  `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`,
  `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,spreadsheets,support`,
  `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,business_commands,business_commands`,
  `business_os_dynamic_packaged_guard_count=13`,
  `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard
  deny/grant/write keys 1, browser warnings/errors/404/request failures 0,
  `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=92`.
- One earlier 11er smoke attempt before Outbound was added was not accepted
  because it hit startup hook reload/budget (`startup_smoke_hook_reload_count=1`,
  `startup_smoke_hook_wait_ms=60200`).
- `node --test src/apps/business-os/modules/customers/customers.test.mjs` was
  attempted separately, but this workspace does not currently install/resolve
  `esbuild` for that module test.

Additional Phase 13C Shiftflow guarded-module evidence passed locally on
2026-06-18:

- `shiftflow` is added to the guarded packaged user-module batch. The Dynamic
  Apps smoke registers `/modules/shiftflow/schema.js` and checks the
  module-owned `planning_shifts` collection.
- Startup seed writes are skipped unless
  `planning_employees`, `planning_projects`, `planning_shifts` and
  `planning_time_records` all have exact write permission.
- Realtime subscriptions now mount through guarded helpers, and the previous
  `globalThis.CTOX_ACTIVE_DB` plus DOM-stashed DB handles were removed.
  Runtime planning actions still use guarded collection-property access, which
  remains explicit in the inventory rather than hidden as a cached-handle
  bypass.
- Remaining source-validated cleanup candidates at this checkpoint are `buchhaltung` and
  `matching`, plus system/internal exception work.
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/shiftflow/index.js`
- `node --check src/apps/business-os/modules/shiftflow/schema.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`
- `python3 -m json.tool src/apps/business-os/modules/shiftflow/module.json >/dev/null`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/modules/shiftflow/test.mjs` passed 4 checks.
- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  completed with existing warnings after the first 14er smoke attempt correctly
  failed fast because the smoke binary did not exist yet.
- Accepted Browser/Rust smoke:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61201 SIGNALING_PORT=61202 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_packaged_guard_module=coding-agents`,
  `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`,
  `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,shiftflow,spreadsheets,support`,
  `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,planning_shifts,business_commands,business_commands`,
  `business_os_dynamic_packaged_guard_count=14`,
  `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard
  deny/grant/write keys 1, browser warnings/errors/404/request failures 0,
  `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=86`.

Additional Phase 13C Buchhaltung guarded-module evidence passed locally on
2026-06-18:

- `buchhaltung` is added to the guarded packaged user-module batch. The Dynamic
  Apps smoke registers `/modules/buchhaltung/schema.js` and checks the
  module-owned `accounting_journal_entries` collection.
- Buchhaltung no longer unwraps `ctx.db.raw`; its `fibuCollection(name)` helper
  resolves `ctx.db.collection(name)` first and only then uses the guarded
  dynamic collection-property fallback.
- The previous `window.ctoxFibuState` global state export was removed, so the
  module no longer exposes `ctx`/`db` through a browser global.
- Automatic Kontenrahmen and demo-data writes are skipped unless the actor has
  exact write grants for the affected accounting collections.
- `accounting_number_series` is now present in schema, manifest and Shell
  fallback metadata, matching the accounting/invoice data model.
- The UI-E2E asset helper now stores its demo asset in in-memory test state
  only, not in localStorage.
- Remaining source-validated cleanup candidate at this checkpoint is `matching`, plus
  system/internal exception work.
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/buchhaltung/index.js`
- `node --check src/apps/business-os/modules/buchhaltung/core/ui_e2e_tests.js`
- `node --check src/apps/business-os/modules/buchhaltung/schema.js`
- `python3 -m json.tool src/apps/business-os/modules/buchhaltung/module.json >/dev/null`
- `python3 -m json.tool src/apps/business-os/modules/buchhaltung/collections.schema.json >/dev/null`
- `node src/apps/business-os/modules/buchhaltung/test.js` passed.
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`
  passed 11 cases.
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- First 15er Browser/Rust attempt passed all feature keys but was not accepted
  because `browser_warning_count=1`; two later attempts were not accepted
  because they hit startup hook reload/budget. Accepted Browser/Rust smoke:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61251 SIGNALING_PORT=61252 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_packaged_guard_module=coding-agents`,
  `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`,
  `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,buchhaltung,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,shiftflow,spreadsheets,support`,
  `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,accounting_journal_entries,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,planning_shifts,business_commands,business_commands`,
  `business_os_dynamic_packaged_guard_count=15`,
  `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard
  deny/grant/write keys 1, browser warnings/errors/404/request failures 0,
  `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=68`.

Additional Phase 13C Matching guarded-module evidence passed locally on
2026-06-18:

- `matching` is added to the guarded packaged user-module batch. The Dynamic
  Apps smoke registers `/modules/matching/schema.js` and checks the
  module-owned `matching_requirements` collection.
- Matching no longer passes `ctx.db.raw` into `businessOsDataSource`, no longer
  imports/opens the CTOX Sync Engine bundle as a standalone fallback, and no longer
  creates missing collections from inside the module.
- `businessOsDataSource` now receives the Shell context through
  `setBusinessOsDatabaseContext(ctx)`, builds its UI-facing aliases on top of
  `ctx.db.collection(name)`, and gates writes through
  `ctx.permissions.canWriteCollection`.
- Unit coverage now verifies normalization through the Shell facade,
  diagnostics through the Shell facade and write denial via the Business OS
  permission facade.
- The first 16er Browser/Rust attempt passed all feature keys but was not
  accepted because the startup hook reloaded once and exceeded the wait budget
  (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60234`).
  The accepted rerun is below.
- `node --check src/apps/business-os/modules/matching/index.js`
- `node --check src/apps/business-os/modules/matching/ui/businessOsDataSource.js`
- `node src/apps/business-os/modules/matching/test.mjs` passed 3 checks.
- `node --check src/apps/business-os/app.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`
  passed 11 cases.
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- Accepted Browser/Rust smoke:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61271 SIGNALING_PORT=61272 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_packaged_guard_module=coding-agents`,
  `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`,
  `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,buchhaltung,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,matching,shiftflow,spreadsheets,support`,
  `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,accounting_journal_entries,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,matching_requirements,planning_shifts,business_commands,business_commands`,
  `business_os_dynamic_packaged_guard_count=16`,
  `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard
  deny/grant/write keys 1, browser warnings/errors/404/request failures 0,
  `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=123`.

Additional Phase 13 system raw-cleanup checkpoint passed locally on
2026-06-18:

- Knowledge, Reports and Tickets no longer call `ctx.db.raw`; their local
  helpers resolve the required system collections through
  `ctx.db.collection(name)`.
- The DB-isolation inventory marks all three as raw-free while preserving
  `system-exception-pending-review`, because their privileged system
  operations still need narrow scoped-policy review before Phase 13 can close.
- At this historical checkpoint the remaining Phase 13 DB-isolation work still
  included Browser/CTOX raw system exceptions, Creator property/proxy access
  and unscoped Shell facades. The follow-up checkpoints below remove the
  Browser/CTOX raw access, the Creator property/proxy access and the unscoped
  Shell/Desktop facades.
- `node -e "JSON.parse(require('fs').readFileSync('docs/business-os-db-isolation-inventory.json','utf8')); console.log('json ok')"`
- `node --check src/apps/business-os/modules/tickets/index.js`
- `node --check src/apps/business-os/modules/reports/index.js`
- `node --check src/apps/business-os/modules/knowledge/index.js`
- Raw DB grep over `src/apps/business-os/modules/tickets`,
  `src/apps/business-os/modules/reports` and
  `src/apps/business-os/modules/knowledge` returned no hits.
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/modules/knowledge/test.mjs`,
  `node src/apps/business-os/modules/reports/test.mjs` and
  `node src/apps/business-os/modules/tickets/tickets-module-smoke.mjs` were
  attempted but blocked before executing because this checkout cannot resolve
  the bare `esbuild` package.
- Browser/Rust `business-os-ui-regression`:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61281 SIGNALING_PORT=61282 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passed with Knowledge primary interactions
  `knowledge-tab-runbooks,knowledge-tab-data,knowledge-tab-skill`, Tickets
  secondary interaction `tickets-search-status-filter`, Reports secondary
  interaction `reports-filter-controls`, browser errors/404/request failures 0
  and `startup_smoke_hook_reload_count=0`. It is not warning-clean:
  `browser_warning_count=1` from the existing Chrome contenteditable/flex
  advisory in the Notes editor path.

Additional Phase 13 Browser/CTOX/Creator DB-cleanup evidence passed locally on
2026-06-18:

- Browser no longer calls `ctx.db.raw`; browser sessions, tabs, frames, input
  events, `business_commands` and `ctox_queue_tasks` resolve through
  `ctx.db.collection(name)`.
- CTOX no longer calls `ctx.db.raw` and no longer falls back to
  `ctx.db.collections`; runtime settings, queue tasks, bug reports and
  command projections resolve through `ctx.db.collection(name)`.
- Creator no longer falls back to `ctx.db` collection properties or
  `ctx.db.collections`; both the Creator runtime and generated app template
  resolve collections through `ctx.db.collection(name)`.
- The DB-isolation inventory marks Browser, CTOX and Creator as raw/property/
  proxy-free while keeping them as explicit system/internal exception surfaces
  until runtime control, browser-frame/input, harness, source-write and
  generated-app install flows have narrow scoped policies or narrow tested
  exceptions.
- Remaining Phase 13 DB-isolation work is now concentrated on the conversion
  of raw-free system/internal surfaces into narrow scoped policies or approved
  system exceptions.
- `node --check src/apps/business-os/modules/browser/index.js`
- `node --check src/apps/business-os/modules/ctox/index.js`
- `node --check src/apps/business-os/modules/creator/index.js`
- Raw/proxy DB grep over `src/apps/business-os/modules/browser`,
  `src/apps/business-os/modules/ctox`,
  `src/apps/business-os/modules/tickets`,
  `src/apps/business-os/modules/reports` and
  `src/apps/business-os/modules/knowledge` returned no hits.
- `node -e "JSON.parse(require('fs').readFileSync('docs/business-os-db-isolation-inventory.json','utf8')); console.log('json ok')"`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- Browser/Rust `business-os-ui-regression`:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61301 SIGNALING_PORT=61302 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passed with primary opened modules `ctox,documents,knowledge,research`,
  primary interactions
  `ctox-zoom,documents-new-drawer,knowledge-tab-runbooks,knowledge-tab-data,knowledge-tab-skill,research-new-task-modal`,
  secondary opened modules
  `matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets`,
  secondary interactions including `browser-address-refresh`,
  `tickets-search-status-filter` and `reports-filter-controls`, browser
  errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`. It
  is still not warning-clean: `browser_warning_count=1` from the existing
  Chrome contenteditable/flex advisory in the Notes editor path.

Additional Phase 13B evidence passed locally on 2026-06-17:

- `node --check src/apps/business-os/app.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18989 SIGNALING_PORT=28989 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`

This smoke now asserts the real Shell context helper path:
`business_os_dynamic_real_context_collection_denied=1`,
`business_os_dynamic_real_context_property_denied=1`,
`business_os_dynamic_real_context_cached_denied=1`,
`business_os_dynamic_real_context_raw_denied=1` and
`business_os_dynamic_real_context_cached_read_grant_allowed=1`. It also seeds a
runtime-installed module into `business_module_catalog`, reloads the Shell,
opens the module through real `openModule(mod)` and requires
`business_os_dynamic_open_module_reload_mounted=1`,
`business_os_dynamic_open_module_collection_denied=1`,
`business_os_dynamic_open_module_property_denied=1`,
`business_os_dynamic_open_module_cached_denied=1` and
`business_os_dynamic_open_module_raw_denied=1`. This closes the Phase 13B
runtime-installed Shell path; later 13E/13F evidence closes runtime safety and
browser-storage scope. At this Phase 13B checkpoint, packaged/core migration
and tested exceptions were still open; later Phase 13C-13I evidence closes
that remainder.

Additional Phase 13F browser-storage-scope evidence passed locally on
2026-06-18:

- `src/apps/business-os/app.js` now exposes scoped storage helpers and
  `ctx.storageScope` (`business-os-storage-scope-v1`) for modules. Shell
  taskbar pins, module layout, account preferences, Shell column/module resizer
  widths and Pairing config use scoped keys. `src/apps/business-os/modules/app-store/index.js`
  uses `ctx.storageScope` for the App Store pane width.
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/app-store/index.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`
- `node src/apps/business-os/scripts/validate-app-module.test.mjs`
- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
- Dynamic Apps Browser/Rust final rerun:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60463 SIGNALING_PORT=60464 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_dynamic_storage_keys_scoped=1`,
  `business_os_dynamic_storage_scope_contract=1`, browser warnings/errors/404/
  request failures 0 and `startup_smoke_hook_reload_count=0`. A prior dynamic
  run passed feature keys but failed only the startup budget after one smoke
  hook reload.
- Audience Browser/Rust:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-audience-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60471 SIGNALING_PORT=60472 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_app_audience_storage_boundary_checked=1`.
- Release Browser/Rust:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60481 SIGNALING_PORT=60482 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  with `business_os_app_release_storage_boundary_checked=1`.

Additional Phase 13 scoped system/internal exception evidence passed locally on
2026-06-18:

- `src/apps/business-os/app.js` routes App Store, Browser, Creator, CTOX,
  Desktop, Knowledge, Reports and Tickets through
  `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS` before the compatibility facade is
  reachable.
- `docs/business-os-db-isolation-inventory.json` marks those modules as
  `system-scoped-exception-tested` or `internal-scoped-exception-tested` and
  stores exact `scoped_collections`.
- `src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` now fails if
  a scoped exception's inventory allowlist diverges from
  `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS`, or if a module remains in
  `*-pending-review` status.
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`
  - 11 cases.
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24
  modules, 5 desktop apps, 0 unscoped facades.
- `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24
  modules.
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  completed with existing warnings.
- Browser/Rust Dynamic Apps:
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-dynamic-apps-system-scope-smoke.json BUSINESS_PORT=61731 SIGNALING_PORT=61732 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passed with `business_os_dynamic_system_scope_modules=app-store,browser,creator,ctox,desktop,knowledge,reports,tickets`,
  `business_os_dynamic_system_scope_count=8`,
  `business_os_dynamic_system_scope_allowed=1`,
  `business_os_dynamic_system_scope_foreign_denied=1`,
  `business_os_dynamic_system_scope_raw_foreign_denied=1`,
  `business_os_dynamic_system_scope_permission_facade=1`,
  `business_os_dynamic_system_scope_capability_contract=1`,
  browser warnings/errors/404/request failures 0,
  `startup_smoke_hook_reload_count=0` and
  `startup_smoke_hook_wait_ms=48`.

Additional Phase 14A smoke-registry evidence passed locally on 2026-06-17:

- `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passes the implemented release mode.

This closes the production smoke-mode/evidence registry and the real release
browser story. The covered audience/dynamic-app browser stories are now also
implemented for Phase 11; agent-scope, auth-scope and fresh-profile are
implemented in the Browser/Rust production smoke path.

Additional Phase 14B auth-scope evidence passed locally on 2026-06-18:

- `node --check src/apps/business-os/app.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-auth-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60721 SIGNALING_PORT=60722 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passes with all `business_os_auth_*` evidence keys true, final state
  `logged_out`, browser warnings/errors/404/request failures 0 and
  `startup_smoke_hook_reload_count=0`.

Additional Phase 14F tenant-boundary evidence passed locally on 2026-06-18:

- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-auth-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-auth-tenant-boundary-smoke.json BUSINESS_PORT=61741 SIGNALING_PORT=61742 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passes with `business_os_auth_cross_scope_storage_denied=1`,
  `business_os_auth_tenant_scope_claim=local-workspace-only`, final state
  `logged_out`, browser warnings/errors/404/request failures 0 and
  `startup_smoke_hook_reload_count=0`.

Additional Phase 14D/14E fresh-profile evidence passed locally on 2026-06-18:

- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-fresh-profile-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60751 SIGNALING_PORT=60752 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passes with all `business_os_fresh_profile_*` evidence keys true, browser
  warnings/errors/404/request failures 0 and
  `startup_smoke_hook_reload_count=0`.

Additional Phase 14F fresh-profile scale evidence passed locally on
2026-06-18:

- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-fresh-profile-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-fresh-profile-scale-smoke.json BUSINESS_PORT=61751 SIGNALING_PORT=61752 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passes with `business_os_fresh_profile_scale_fixture_modules=32`,
  `business_os_fresh_profile_scale_catalog_modules=57`,
  `business_os_fresh_profile_scale_explicit_grants=64`,
  `business_os_fresh_profile_scale_release_versions=96`,
  `business_os_fresh_profile_scale_native_audit_events=128`,
  `business_os_fresh_profile_scale_app_store_cards=32`, render 8 ms,
  start menu 10 ms, App Store 116 ms, browser warnings/errors/404/request
  failures 0 and `startup_smoke_hook_wait_ms=36`.
- Full production Browser/Rust matrix
  `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES="$(node -e "const { businessOsProductionSmokeModes } = require('./src/core/rxdb/tools/business_os_production_smoke_registry'); process.stdout.write(businessOsProductionSmokeModes.join(','));")" SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_BROWSER_WARNING_BUDGET=0 SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-production-smoke-phase14-scale-summary.json BUSINESS_PORT=61761 SIGNALING_PORT=61762 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  passes Release, Audience, Agent Scope, Auth Scope and Fresh Profile with
  browser warnings/request failures/startup reloads 0.

Additional Phase 10F/10H release evidence passed locally on 2026-06-17:

- `node src/apps/business-os/modules/app-store/app-store.test.mjs`
- `node src/apps/business-os/shared/app-lifecycle.test.mjs`
- `node src/apps/business-os/shared/react-settings.test.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`
- `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`
- `cargo fmt --check`
- `CARGO_TARGET_DIR=/Users/you/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo build --bin ctox`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`

The release smoke asserts `business_os_app_release_private_before_release=1`,
`business_os_app_release_publish_succeeded=1`,
`business_os_app_release_team_visible_after_release=1`,
`business_os_app_release_version_badge_visible=1`,
`business_os_app_release_data_review_visible=1`,
`business_os_app_release_rollback_succeeded=1`,
`business_os_app_release_release_audit_visible=1`,
`business_os_app_release_rollback_audit_visible=1`,
`business_os_app_release_activity_audit_redacted=1`,
`business_os_app_release_reload_verified=1` and
`business_os_app_release_storage_boundary_checked=1`, with
`browser_warning_count=0`, `browser_error_count=0`,
`browser_resource_404_count=0` and `browser_request_failure_count=0`.

## Phase 10 Backend Evidence

Passed locally on 2026-06-17 for the native release projection sub-slice:

- `cargo test --bin ctox module_catalog_projects_release_state_data_access_and_rollback_target -- --nocapture`
- `cargo test --bin ctox module_release_ -- --nocapture`
- `cargo test --bin ctox module_ -- --nocapture`
- `rustfmt --edition 2021 src/core/business_os/store.rs --check`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `cargo check --bin ctox`
- `cargo test --manifest-path src/core/rxdb/Cargo.toml`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`

Passed locally on 2026-06-17 for the Phase 10D2 backend consistency sub-slice:

- `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/rxdb_peer.rs --check`
- `cargo test --bin ctox module_release_command_replay_does_not_duplicate_release_state --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
- `cargo test --bin ctox module_lifecycle_projection_repair_resyncs_releases_and_catalog --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
- `cargo test --bin ctox module_release_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
- `cargo test --bin ctox module_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `cargo check --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`

This closes backend duplicate-command and lifecycle projection repair evidence
for release rows/catalog. It did not by itself replace the App Store publish
wizard, release reload smoke or Settings Activity/browser audit labels; those
Phase 10 browser/UI points are closed by the later release smoke evidence.
Operational recovery drills and CI release-gate work remain Phase 15/16.

This is backend/static evidence only. It is not a substitute for the remaining
Browser/Rust release smoke, auth/tenant/fresh-profile
evidence or CI release-gate work tracked in
`docs/business-os-roles-permissions-plan.md`. Settings fallback alignment is
closed separately as read-only diagnostics with Browser/Rust evidence.

Passed locally on 2026-06-17 for the Phase 10E3 UI/static partial sub-slice:

- `node src/apps/business-os/shared/app-lifecycle.test.mjs`
- `node src/apps/business-os/modules/app-store/app-store.test.mjs`
- `node src/apps/business-os/shared/react-settings.test.mjs`
- `node --check src/apps/business-os/shared/app-lifecycle.js`
- `node --check src/apps/business-os/modules/app-store/index.js`
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/shared/react-settings.js`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `git diff --check --` touched Phase-10E3 JS/CSS/docs paths
- trailing-whitespace grep over touched Phase-10E3 JS/CSS/docs paths

Historical checkpoint: this was not full Phase 10E3 completion yet. The later
`business-os-app-release-ui` Browser/Rust smoke closes reload evidence,
"success only after projected state" proof and release-browser evidence.

Passed locally on 2026-06-17 for the Phase 10G Settings fallback static
downgrade:

- `node src/apps/business-os/shared/react-settings.test.mjs`
- `node --check src/apps/business-os/shared/react-settings.js`
- static source guard in `react-settings.test.mjs` proving no active Settings
  release/rollback dispatch path or stale release/rollback `data-*` controls
  are exposed.

The static downgrade alone was not full Phase 10G completion because the real
Settings drawer still needed Browser/Rust disabled/read-only evidence.

Passed locally on 2026-06-17 for Phase 10G Settings fallback Browser/Rust
proof:

- `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18988 SIGNALING_PORT=28988 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- Matrix evidence: `business_os_roles_permissions_settings_release_fallback_readonly=1`,
  `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=79`,
  `browser_error_count=0`, `browser_resource_404_count=0` and
  `browser_request_failure_count=0`.

Phase 10G Settings fallback is now closed; Phase 10D2 closes backend
projection/repair consistency. The later App Store release and Activity
browser evidence closes the remaining Phase 10 implementation work. CI/release
gate integration remains tracked in Phase 16; the local smoke artifact schema
slice is closed, while CI upload/retention and release-workflow gating remain
open.

Passed locally on 2026-06-17 for the Phase 10F App Store release UI/payload
partial:

- `node --check src/apps/business-os/modules/app-store/index.js`
- `node src/apps/business-os/modules/app-store/app-store.test.mjs`
- `git diff --check -- src/apps/business-os/modules/app-store/index.js src/apps/business-os/modules/app-store/index.css src/apps/business-os/modules/app-store/app-store.test.mjs`

This adds the permission-gated `Freigeben` action and release dialog payload
for `ctox.module.release`; it is not the Browser/Rust release-smoke proof.

Passed locally on 2026-06-17 for Phase 11D launcher badge and lifecycle drawer
permission UX:

- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/shared/shell-permissions-ui.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs`
- `node --test src/apps/business-os/shared/app-lifecycle.test.mjs`
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html /Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node src/core/rxdb/tools/browser_rust_smoke.js`
- Smoke evidence: `business_os_dynamic_launcher_badges_visible=1`,
  `business_os_dynamic_lifecycle_drawer_manager_state=1`,
  `business_os_dynamic_lifecycle_drawer_readonly_state=1`,
  `browser_warning_count=0`, `browser_error_count=0`,
  `browser_resource_404_count=0` and `browser_request_failure_count=0`.

Passed locally on 2026-06-17 for Phase 12A MCP app visibility/data split:

- `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs`
- `cargo test --bin ctox mcp_business_os_policy -- --nocapture`
- Test coverage includes visible app without `data.read`, hidden app despite
  `data.read`, hidden app execution denied despite `data.write`, module link
  allowed with `apps.view` without `data.read`, and Team default visibility for
  `1.0.0` runtime-installed apps.

Passed locally on 2026-06-17 for Phase 12B/12C first right-click and command
context slice:

- Read-only subagent Galileo verified the current right-click, Business Chat,
  Coding Agents and command-bus context gaps before implementation.
- `node --check src/apps/business-os/shared/shell-permissions-ui.js`
- `node --check src/apps/business-os/shared/command-bus.js`
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/coding-agents/index.js`
- `node --check src/apps/business-os/shared/business-chat.js`
- `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs`
- `node --test src/apps/business-os/shared/command-bus.test.mjs`
- `git diff --check -- src/apps/business-os/shared/shell-permissions-ui.js src/apps/business-os/shared/shell-permissions-ui.test.mjs src/apps/business-os/shared/command-bus.js src/apps/business-os/shared/command-bus.test.mjs src/apps/business-os/shared/business-chat.js src/apps/business-os/app.js src/apps/business-os/app.css src/apps/business-os/modules/coding-agents/index.js`

Passed locally on 2026-06-18 for Phase 12B/12C Agent Scope Browser/Rust proof
covering global right-click, App Store context-chat and Business Chat rendered
scope:

- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/app-store/index.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke.js`
- `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`
- `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed
- `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed
- `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 7 passed
- `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed
- `PLAYWRIGHT_MODULE_PATH=/Users/you/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60241 SIGNALING_PORT=60242 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
- Smoke evidence: `business_os_agent_scope_panel_visible=1`,
  `business_os_agent_scope_client_context_matches_ui=1`,
  `business_os_agent_scope_app_store_panel_visible=1`,
  `business_os_agent_scope_app_store_context_matches_ui=1`,
  `business_os_agent_scope_business_chat_scope_matches_context=1`,
  `business_os_agent_scope_settings_grant_boundary_visible=1`,
  `business_os_agent_scope_app_hidden_denied=1`,
  `business_os_agent_scope_data_denied_before_grant=1`,
  `business_os_agent_scope_read_allowed_after_grant=1`,
  `business_os_agent_scope_write_denied_without_grant=1`,
  `business_os_agent_scope_audit_visible=1`,
  `browser_warning_count=0`, `browser_error_count=0`,
  `browser_resource_404_count=0` and `browser_request_failure_count=0`.

## Acceptance Criteria

- Users can understand app state from the icon without opening Settings.
- Version and visibility state are always visible at the point of app choice.
- Private `0.x` apps do not leak into Team launcher/App Store views.
- `1.0.0+` apps are Team-visible by default unless explicitly restricted.
- App visibility never implies unrestricted data access.
- MCP module visibility is not derived from data grants.
- Browser modules cannot bypass `data.read`/`data.write` through `ctx.db.raw`.
- Global right-click AI context shows actor/app/selection/data/external scope
  before submit and submits the same visible scope in `client_context`.
- KI/Agent access uses the same role/grant model as human users.
- All changes remain RxDB/WebRTC-only and do not introduce HTTP data fallbacks.
