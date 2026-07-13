#!/usr/bin/env node
/*
 * Serial full-app Browser/Rust RxDB WebRTC smoke matrix.
 *
 * Requires a built CTOX binary at CTOX_BIN or the default integration target:
 *   cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target
 *
 * Run the default full-app matrix:
 *   node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *
 * Run selected modes:
 *   SMOKE_MODES=rust-to-browser,workspace-rust-to-browser,workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-agent-artifacts-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-agent-artifacts-stress-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-agent-artifacts-churn-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-agent-artifacts-background-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=migration-version-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=tickets-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=browser-lifecycle-ui SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=browser-handoff-ui SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=coding-agents-ui SMOKE_PAGE_PATH=/index.html SMOKE_CODING_AGENT_PROVIDER=codex node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=business-os-roles-permissions-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=business-os-app-release-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=business-os-app-audience-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=business-os-agent-scope-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=business-os-auth-scope-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=business-os-fresh-profile-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-smoke-matrix-summary.json SMOKE_MODES=business-os-agent-scope-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-burst-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-reload-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=command-midflight-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=rollover-native-peer-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=tab-freeze-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=network-flap-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=signaling-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=checkpoint-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=schema-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=replication-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=replication-push-contract-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=file-chunk-metadata-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=file-chunk-tombstone-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 *   SMOKE_MODES=file-chunk-stale-generation-error-browser-status node src/core/rxdb/tools/browser_rust_smoke_matrix.js
 */
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const { spawnSync } = require('child_process');
const os = require('os');
const {
  assertBusinessOsProductionSmokeRegistry,
  businessOsProductionSmokeEvidenceRequirements,
  businessOsProductionSmokeModes,
  businessOsProductionSmokeModeSet,
} = require('./business_os_production_smoke_registry');

const toolPath = path.join(__dirname, 'browser_rust_smoke.js');
const root = path.resolve(__dirname, '../../../..');
const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const SMOKE_MATRIX_SUMMARY_SCHEMA = 'ctox.business_os.smoke_matrix_summary.v1';
const defaultResultPath = path.join(root, 'runtime/build/business-os-smoke-matrix-summary.json');
const defaultModes = [
  'rust-to-browser',
  'browser-to-rust',
  'command-browser-to-rust',
  'tickets-browser-to-rust',
  'business-os-ui-regression',
  'business-os-roles-permissions-ui',
  'business-os-dynamic-apps-ui',
  'business-os-app-release-ui',
  'business-os-app-audience-ui',
  'browser-lifecycle-ui',
  'browser-handoff-ui',
  'migration-version-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-reload-browser-to-rust',
  'command-restart-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
  'office-document-midflight-restart-browser-to-rust',
  'office-spreadsheet-midflight-restart-browser-to-rust',
  'restart-browser-to-rust',
  'restart-signaling-browser-to-rust',
  'rollover-native-peer-browser-to-rust',
  'tab-freeze-browser-to-rust',
  'network-flap-browser-to-rust',
  'signaling-error-browser-status',
  'peer-lifecycle-browser-status',
  'checkpoint-error-browser-status',
  'rxdb-protocol-error-browser-status',
  'schema-error-browser-status',
  'replication-error-browser-status',
  'replication-push-contract-error-browser-status',
  'file-chunk-metadata-error-browser-status',
  'file-chunk-tombstone-error-browser-status',
  'file-chunk-stale-generation-error-browser-status',
  'workspace-rust-to-browser',
  'workspace-agent-artifacts-rust-to-browser',
  'workspace-agent-artifacts-stress-rust-to-browser',
  'workspace-agent-artifacts-churn-rust-to-browser',
  'workspace-agent-artifacts-background-rust-to-browser',
  'workspace-update-rust-to-browser',
  'workspace-large-materialize-rust-to-browser',
  'workspace-large-file-viewer-rust-to-browser',
  'workspace-large-file-viewer-restart-rust-to-browser',
];
const modeEvidenceRequirements = {
  'rust-to-browser': { keys: ['replicated_id'] },
  'browser-to-rust': { keys: ['readiness_payload', 'replicated_id'] },
  'command-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status'],
  },
  'tickets-browser-to-rust': {
    keys: ['command_id', 'task_id', 'status', 'ticket_key', 'ticket_source', 'ticket_title'],
    values: {
      status: 'completed',
      ticket_source: 'local',
    },
  },
  'coding-agents-ui': {
    keys: [
      'coding_agents_ui_provider',
      'coding_agents_ui_workspace_root',
      'coding_agents_ui_session_id',
      'coding_agents_ui_status_ready',
      'coding_agents_ui_create_marker_seen',
      'coding_agents_ui_followup_marker_seen',
      'coding_agents_ui_session_projection_status',
      'coding_agents_ui_event_count',
      'coding_agents_ui_user_event_count',
      'coding_agents_ui_assistant_event_count',
      'coding_agents_ui_active_module',
    ],
    minimums: {
      coding_agents_ui_event_count: 4,
      coding_agents_ui_user_event_count: 2,
      coding_agents_ui_assistant_event_count: 2,
    },
    values: {
      coding_agents_ui_status_ready: 1,
      coding_agents_ui_create_marker_seen: 1,
      coding_agents_ui_followup_marker_seen: 1,
      coding_agents_ui_active_module: 'coding-agents',
    },
  },
  'business-os-ui-regression': {
    keys: [
      'business_os_ui_module_count',
      'business_os_ui_start_menu_items',
      'business_os_ui_opened_modules',
      'business_os_ui_rendered_modules',
      'business_os_ui_interacted_modules',
      'business_os_ui_interaction_names',
      'business_os_ui_interaction_actions',
      'business_os_ui_min_module_text_length',
      'business_os_ui_secondary_opened_modules',
      'business_os_ui_secondary_rendered_modules',
      'business_os_ui_secondary_interacted_modules',
      'business_os_ui_secondary_interaction_names',
      'business_os_ui_min_secondary_text_length',
      'business_os_ui_desktop_opened',
      'business_os_visual_workspace_visible',
      'business_os_visual_desktop_icon_count',
      'business_os_visual_screenshot_unique_colors',
      'business_os_visual_screenshot_luma_stddev',
    ],
    minimums: {
      business_os_ui_module_count: 8,
      business_os_ui_start_menu_items: 8,
      business_os_ui_min_module_text_length: 40,
      business_os_ui_min_secondary_text_length: 30,
      business_os_visual_desktop_icon_count: 6,
      business_os_visual_screenshot_unique_colors: 48,
      business_os_visual_screenshot_luma_stddev: 8,
    },
    values: {
      business_os_ui_rendered_modules: 'ctox,documents,knowledge,research',
      business_os_ui_secondary_opened_modules: 'matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets,appsec-pentest,consent,credentials,customers,cv-print-builder,esign,intake,interviews,iot,nachweise,placements,submissions,support,threads',
      business_os_ui_secondary_rendered_modules: 'matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets,appsec-pentest,consent,credentials,customers,cv-print-builder,esign,intake,interviews,iot,nachweise,placements,submissions,support,threads',
      business_os_ui_secondary_interacted_modules: 'matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets,appsec-pentest,consent,credentials,customers,cv-print-builder,esign,intake,interviews,iot,nachweise,placements,submissions,support,threads',
      business_os_ui_secondary_interaction_names: 'matching-list-matrix-tabs,policy-denied-render-skipped,policy-denied-render-skipped,tickets-search-status-filter,shiftflow-center-tabs,buchhaltung-nav-switch,policy-denied-render-skipped,app-store-view-scope,browser-address-refresh,policy-denied-render-skipped,creator-expert-accordion,policy-denied-render-skipped,reports-filter-controls,policy-denied-render-skipped,appsec-coverage-findings-tabs,consent-primary-form-input,credentials-write-only-form-input,policy-denied-render-skipped,policy-denied-render-skipped,esign-primary-form-input,intake-primary-form-input,interviews-primary-form-input,policy-denied-render-skipped,nachweise-primary-form-input,placements-primary-form-input,submissions-primary-form-input,policy-denied-render-skipped,threads-search-and-filter',
      business_os_ui_interacted_modules: 'ctox,documents,knowledge,research',
      business_os_ui_interaction_names: 'ctox-zoom,documents-new-drawer,knowledge-tab-runbooks,knowledge-tab-data,knowledge-tab-skill,research-new-task-modal',
      business_os_ui_desktop_opened: 1,
      business_os_visual_workspace_visible: 1,
    },
  },
  'business-os-roles-permissions-ui': {
    keys: [
      'business_os_roles_permissions_target_module',
      'business_os_roles_permissions_other_module',
      'business_os_roles_permissions_team_modify_hidden',
      'business_os_roles_permissions_team_source_hidden',
      'business_os_roles_permissions_source_grant_visible',
      'business_os_roles_permissions_modify_grant_visible',
      'business_os_roles_permissions_owner_context_visible',
      'business_os_roles_permissions_appbar_source_gate',
      'business_os_roles_permissions_exact_scope_isolated',
      'business_os_roles_permissions_owner_role_option',
      'business_os_roles_permissions_admin_owner_option_hidden',
      'business_os_roles_permissions_business_labels',
      'business_os_roles_permissions_settings_release_fallback_readonly',
      'business_os_roles_permissions_settings_why_diagnostics_visible',
      'business_os_roles_permissions_settings_why_diagnostics_rows',
      'business_os_roles_permissions_settings_why_diagnostics_redacted',
      'business_os_roles_permissions_settings_support_diagnostics_visible',
      'business_os_roles_permissions_settings_support_diagnostics_rows',
      'business_os_roles_permissions_settings_support_diagnostics_redacted',
      'business_os_roles_permissions_settings_support_diagnostics_download',
      'business_os_roles_permissions_reload_verified',
      'business_os_roles_permissions_auth_state',
      'advanced_status',
    ],
    values: {
      business_os_roles_permissions_team_modify_hidden: 1,
      business_os_roles_permissions_team_source_hidden: 1,
      business_os_roles_permissions_source_grant_visible: 1,
      business_os_roles_permissions_modify_grant_visible: 1,
      business_os_roles_permissions_owner_context_visible: 1,
      business_os_roles_permissions_appbar_source_gate: 1,
      business_os_roles_permissions_exact_scope_isolated: 1,
      business_os_roles_permissions_owner_role_option: 1,
      business_os_roles_permissions_admin_owner_option_hidden: 1,
      business_os_roles_permissions_business_labels: 1,
      business_os_roles_permissions_settings_release_fallback_readonly: 1,
      business_os_roles_permissions_settings_why_diagnostics_visible: 1,
      business_os_roles_permissions_settings_why_diagnostics_rows: 1,
      business_os_roles_permissions_settings_why_diagnostics_redacted: 1,
      business_os_roles_permissions_settings_support_diagnostics_visible: 1,
      business_os_roles_permissions_settings_support_diagnostics_rows: 1,
      business_os_roles_permissions_settings_support_diagnostics_redacted: 1,
      business_os_roles_permissions_settings_support_diagnostics_download: 1,
      business_os_roles_permissions_reload_verified: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
  },
  'business-os-dynamic-apps-ui': {
    keys: [
      'business_os_dynamic_private_module',
      'business_os_dynamic_team_module',
      'business_os_dynamic_private_hidden_for_team',
      'business_os_dynamic_private_visible_for_builder',
      'business_os_dynamic_team_visible_for_released',
      'business_os_dynamic_restricted_hidden_for_team',
      'business_os_dynamic_lifecycle_badges_visible',
      'business_os_dynamic_lifecycle_drawer_visible',
      'business_os_dynamic_lifecycle_why_diagnostics_visible',
      'business_os_dynamic_lifecycle_why_diagnostics_rows',
      'business_os_dynamic_lifecycle_why_diagnostics_data',
      'business_os_dynamic_db_read_denied',
      'business_os_dynamic_db_raw_denied',
      'business_os_dynamic_real_context_collection_denied',
      'business_os_dynamic_real_context_property_denied',
      'business_os_dynamic_real_context_cached_denied',
      'business_os_dynamic_real_context_raw_denied',
      'business_os_dynamic_open_module_reload_mounted',
      'business_os_dynamic_open_module_collection_denied',
      'business_os_dynamic_open_module_property_denied',
      'business_os_dynamic_open_module_cached_denied',
      'business_os_dynamic_open_module_raw_denied',
      'business_os_dynamic_runtime_safety_contract',
      'business_os_dynamic_runtime_safety_capabilities',
      'business_os_dynamic_storage_keys_scoped',
      'business_os_dynamic_storage_scope_contract',
      'business_os_dynamic_db_read_grant_allowed',
      'business_os_dynamic_real_context_cached_read_grant_allowed',
      'business_os_dynamic_db_write_denied_without_write',
      'business_os_dynamic_permission_facade_read_allowed',
      'business_os_dynamic_permission_facade_write_denied',
      'business_os_dynamic_packaged_guard_module',
      'business_os_dynamic_packaged_guard_collection',
      'business_os_dynamic_packaged_guard_capability_contract',
      'business_os_dynamic_packaged_guard_read_denied',
      'business_os_dynamic_packaged_guard_property_denied',
      'business_os_dynamic_packaged_guard_raw_denied',
      'business_os_dynamic_packaged_guard_context_denied',
      'business_os_dynamic_packaged_guard_context_property_denied',
      'business_os_dynamic_packaged_guard_read_grant_allowed',
      'business_os_dynamic_packaged_guard_context_permission_facade',
      'business_os_dynamic_packaged_guard_write_denied_without_write',
      'business_os_dynamic_packaged_guard_shell_locked_state',
      'business_os_dynamic_packaged_guard_modules',
      'business_os_dynamic_packaged_guard_collections',
      'business_os_dynamic_packaged_guard_count',
      'business_os_dynamic_packaged_guard_batch_coverage',
      'business_os_dynamic_packaged_guard_all_capability_contracts',
      'business_os_dynamic_packaged_guard_all_read_denied',
      'business_os_dynamic_packaged_guard_all_property_denied',
      'business_os_dynamic_packaged_guard_all_raw_denied',
      'business_os_dynamic_packaged_guard_all_context_denied',
      'business_os_dynamic_packaged_guard_all_read_grants_allowed',
      'business_os_dynamic_packaged_guard_all_context_permission_facades',
      'business_os_dynamic_packaged_guard_all_writes_denied_without_write',
      'business_os_dynamic_system_scope_modules',
      'business_os_dynamic_system_scope_count',
      'business_os_dynamic_system_scope_allowed',
      'business_os_dynamic_system_scope_foreign_denied',
      'business_os_dynamic_system_scope_raw_foreign_denied',
      'business_os_dynamic_system_scope_permission_facade',
      'business_os_dynamic_system_scope_capability_contract',
      'business_os_dynamic_invalid_version_private',
      'business_os_dynamic_reload_verified',
      'business_os_dynamic_auth_state',
      'advanced_status',
    ],
    values: {
      business_os_dynamic_private_hidden_for_team: 1,
      business_os_dynamic_private_visible_for_builder: 1,
      business_os_dynamic_team_visible_for_released: 1,
      business_os_dynamic_restricted_hidden_for_team: 1,
      business_os_dynamic_lifecycle_badges_visible: 1,
      business_os_dynamic_lifecycle_drawer_visible: 1,
      business_os_dynamic_lifecycle_why_diagnostics_visible: 1,
      business_os_dynamic_lifecycle_why_diagnostics_rows: 1,
      business_os_dynamic_lifecycle_why_diagnostics_data: 1,
      business_os_dynamic_db_read_denied: 1,
      business_os_dynamic_db_raw_denied: 1,
      business_os_dynamic_real_context_collection_denied: 1,
      business_os_dynamic_real_context_property_denied: 1,
      business_os_dynamic_real_context_cached_denied: 1,
      business_os_dynamic_real_context_raw_denied: 1,
      business_os_dynamic_open_module_reload_mounted: 1,
      business_os_dynamic_open_module_collection_denied: 1,
      business_os_dynamic_open_module_property_denied: 1,
      business_os_dynamic_open_module_cached_denied: 1,
      business_os_dynamic_open_module_raw_denied: 1,
      business_os_dynamic_runtime_safety_contract: 1,
      business_os_dynamic_runtime_safety_capabilities: 1,
      business_os_dynamic_storage_keys_scoped: 1,
      business_os_dynamic_storage_scope_contract: 1,
      business_os_dynamic_db_read_grant_allowed: 1,
      business_os_dynamic_real_context_cached_read_grant_allowed: 1,
      business_os_dynamic_db_write_denied_without_write: 1,
      business_os_dynamic_permission_facade_read_allowed: 1,
      business_os_dynamic_permission_facade_write_denied: 1,
      business_os_dynamic_packaged_guard_module: 'coding-agents',
      business_os_dynamic_packaged_guard_collection: 'coding_agent_sessions',
      business_os_dynamic_packaged_guard_capability_contract: 1,
      business_os_dynamic_packaged_guard_read_denied: 1,
      business_os_dynamic_packaged_guard_property_denied: 1,
      business_os_dynamic_packaged_guard_raw_denied: 1,
      business_os_dynamic_packaged_guard_context_denied: 1,
      business_os_dynamic_packaged_guard_context_property_denied: 1,
      business_os_dynamic_packaged_guard_read_grant_allowed: 1,
      business_os_dynamic_packaged_guard_context_permission_facade: 1,
      business_os_dynamic_packaged_guard_write_denied_without_write: 1,
      business_os_dynamic_packaged_guard_shell_locked_state: 1,
      business_os_dynamic_packaged_guard_modules: 'coding-agents,calendar,buchhaltung,conversations,customers,cv-print-builder,invoices,iot,notes,outbound,matching,shiftflow,spreadsheets,support',
      business_os_dynamic_packaged_guard_collections: 'coding_agent_sessions,calendar_events,accounting_journal_entries,business_commands,customer_accounts,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,matching_requirements,planning_shifts,business_commands,business_commands',
      business_os_dynamic_packaged_guard_count: 14,
      business_os_dynamic_packaged_guard_batch_coverage: 1,
      business_os_dynamic_packaged_guard_all_capability_contracts: 1,
      business_os_dynamic_packaged_guard_all_read_denied: 1,
      business_os_dynamic_packaged_guard_all_property_denied: 1,
      business_os_dynamic_packaged_guard_all_raw_denied: 1,
      business_os_dynamic_packaged_guard_all_context_denied: 1,
      business_os_dynamic_packaged_guard_all_read_grants_allowed: 1,
      business_os_dynamic_packaged_guard_all_context_permission_facades: 1,
      business_os_dynamic_packaged_guard_all_writes_denied_without_write: 1,
      business_os_dynamic_system_scope_modules: 'app-store,browser,creator,ctox,desktop,documents,knowledge,research,reports,tickets',
      business_os_dynamic_system_scope_count: 10,
      business_os_dynamic_system_scope_allowed: 1,
      business_os_dynamic_system_scope_foreign_denied: 1,
      business_os_dynamic_system_scope_raw_foreign_denied: 1,
      business_os_dynamic_system_scope_permission_facade: 1,
      business_os_dynamic_system_scope_capability_contract: 1,
      business_os_dynamic_invalid_version_private: 1,
      business_os_dynamic_reload_verified: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
  },
  ...businessOsProductionSmokeEvidenceRequirements,
  'browser-lifecycle-ui': {
    keys: [
      'browser_lifecycle_command_count',
      'browser_lifecycle_command_types',
      'browser_lifecycle_accepted_types',
      'browser_lifecycle_session_status',
      'browser_lifecycle_runtime_status',
      'browser_lifecycle_tab_status',
      'browser_lifecycle_last_command',
    ],
    values: {
      browser_lifecycle_command_count: 7,
      browser_lifecycle_command_types: 'browser.session.start,browser.navigate,browser.reload,browser.back,browser.forward,browser.reset,browser.session.stop',
      browser_lifecycle_accepted_types: 'browser.session.start,browser.navigate,browser.reload,browser.back,browser.forward,browser.reset,browser.session.stop',
      browser_lifecycle_session_status: 'stopped',
      browser_lifecycle_runtime_status: 'stopped',
      browser_lifecycle_tab_status: 'stopped',
      browser_lifecycle_last_command: 'browser.session.stop',
    },
  },
  'browser-handoff-ui': {
    keys: [
      'browser_handoff_command_id',
      'browser_handoff_command_status',
      'browser_handoff_command_type',
      'browser_handoff_task_id',
      'browser_handoff_task_status',
      'browser_handoff_task_inbound_channel',
      'browser_handoff_frame_id',
      'browser_handoff_frame_seq',
      'browser_handoff_visible',
    ],
    values: {
      browser_handoff_command_status: 'accepted',
      browser_handoff_command_type: 'ctox.browser_context.capture',
      browser_handoff_task_inbound_channel: 'browser',
      browser_handoff_visible: 1,
    },
  },
  'migration-version-browser-to-rust': {
    keys: [
      'schema_collection',
      'schema_version',
      'schema_table',
      'stale_schema_table',
      'stale_schema_table_rows',
      'task_table',
      'command_id',
      'task_id',
      'task_count_for_command',
      'status',
      'task_status',
    ],
    values: {
      schema_collection: 'business_commands',
      schema_version: 1,
      schema_table: 'ctox_business_os__business_commands__v1',
      stale_schema_table: 'ctox_business_os__business_commands__v0',
      stale_schema_table_rows: 0,
      task_table: 'ctox_business_os__ctox_queue_tasks__v0',
    },
  },
  'command-burst-browser-to-rust': {
    keys: ['command_count', 'task_count_for_commands', 'command_ids', 'task_ids'],
  },
  'command-reload-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status', 'reload_verified'],
    values: { reload_verified: 1 },
  },
  'command-restart-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status'],
  },
  'command-midflight-restart-browser-to-rust': {
    keys: ['command_id', 'task_id', 'task_count_for_command', 'status', 'task_status'],
  },
  'office-document-midflight-restart-browser-to-rust': {
    keys: ['command_id', 'task_count_for_command', 'status', 'task_status', 'office_kind', 'office_record_id', 'office_base_version_id', 'office_committed_version_id', 'office_canonical_blob_id', 'office_canonical_blob_chunk_count'],
    values: { status: 'completed', task_status: 'completed', task_count_for_command: 0, office_kind: 'document' },
    minimums: { office_canonical_blob_chunk_count: 1 },
  },
  'office-spreadsheet-midflight-restart-browser-to-rust': {
    keys: ['command_id', 'task_count_for_command', 'status', 'task_status', 'office_kind', 'office_record_id', 'office_base_version_id', 'office_committed_version_id', 'office_canonical_blob_id', 'office_canonical_blob_chunk_count'],
    values: { status: 'completed', task_status: 'completed', task_count_for_command: 0, office_kind: 'spreadsheet' },
    minimums: { office_canonical_blob_chunk_count: 1 },
  },
  'restart-browser-to-rust': {
    keys: ['advanced_status', 'checkpoint_restarted_collections', 'checkpoint_epoch_count', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
    minimums: { checkpoint_epoch_count: 1 },
  },
  'restart-signaling-browser-to-rust': {
    keys: ['advanced_status', 'checkpoint_restarted_collections', 'checkpoint_epoch_count', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
    minimums: { checkpoint_epoch_count: 1 },
  },
  'rollover-native-peer-browser-to-rust': {
    keys: ['advanced_status', 'checkpoint_restarted_collections', 'checkpoint_epoch_count', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
    minimums: { checkpoint_epoch_count: 1 },
  },
  'tab-freeze-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'network-flap-browser-to-rust': {
    keys: ['advanced_status', 'readiness_payload', 'replicated_id'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
  'signaling-error-browser-status': {
    keys: ['signaling_error_collection', 'signaling_error_code', 'signaling_error_name'],
    values: {
      signaling_error_code: 'instance_mismatch',
      signaling_error_name: 'CtoxSignalingControlPlaneError',
    },
  },
  'peer-lifecycle-browser-status': {
    keys: ['peer_lifecycle_collection', 'peer_lifecycle_code', 'peer_lifecycle_name', 'peer_lifecycle_phase'],
    values: {
      peer_lifecycle_code: 'peer_connection_lost',
      peer_lifecycle_name: 'CtoxWebRtcPeerLifecycleEvent',
      peer_lifecycle_phase: 'peer-reconnect',
    },
  },
  'checkpoint-error-browser-status': {
    keys: ['checkpoint_error_collection', 'checkpoint_error_code', 'checkpoint_error_name'],
    values: {
      checkpoint_error_code: 'ctox_checkpoint_epoch_missing',
      checkpoint_error_name: 'CtoxCheckpointProtocolError',
    },
  },
  'rxdb-protocol-error-browser-status': {
    keys: ['rxdb_protocol_error_collection', 'rxdb_protocol_error_code', 'rxdb_protocol_error_name'],
    values: {
      rxdb_protocol_error_code: 'ctox_rxdb_protocol_mismatch',
      rxdb_protocol_error_name: 'CtoxSchemaProtocolError',
    },
  },
  'schema-error-browser-status': {
    keys: ['schema_error_collection', 'schema_error_code', 'schema_error_name'],
    values: {
      schema_error_code: 'ctox_schema_hash_mismatch',
      schema_error_name: 'CtoxSchemaProtocolError',
    },
  },
  'replication-error-browser-status': {
    keys: ['replication_error_collection', 'replication_error_code', 'replication_error_name'],
    values: {
      replication_error_code: 'ctox_replication_pull_failed',
      replication_error_name: 'CtoxReplicationIoError',
    },
  },
  'replication-push-contract-error-browser-status': {
    keys: ['replication_push_error_collection', 'replication_push_error_code', 'replication_push_error_name'],
    values: {
      replication_push_error_code: 'ctox_replication_push_contract_invalid',
      replication_push_error_name: 'CtoxReplicationIoError',
    },
  },
  'file-chunk-metadata-error-browser-status': {
    keys: ['file_integrity_error_name', 'file_integrity_error_code', 'file_integrity_error_phase', 'replicated_id'],
    values: {
      file_integrity_error_name: 'CtoxFileChunkIntegrityError',
      file_integrity_error_code: 'ctox_file_chunk_integrity_mismatch',
      file_integrity_error_phase: 'file-chunk-reconstruct',
    },
  },
  'file-chunk-tombstone-error-browser-status': {
    keys: ['file_integrity_error_name', 'file_integrity_error_code', 'file_integrity_error_phase', 'replicated_id', 'live_chunk_count'],
    values: {
      file_integrity_error_name: 'CtoxFileChunkIntegrityError',
      file_integrity_error_code: 'ctox_file_chunk_integrity_mismatch',
      file_integrity_error_phase: 'file-chunk-reconstruct',
      live_chunk_count: 0,
    },
  },
  'file-chunk-stale-generation-error-browser-status': {
    keys: [
      'file_integrity_error_name',
      'file_integrity_error_code',
      'file_integrity_error_phase',
      'replicated_id',
      'requested_generation',
      'requested_generation_chunk_count',
      'live_chunk_count',
      'available_generations',
    ],
    values: {
      file_integrity_error_name: 'CtoxFileChunkIntegrityError',
      file_integrity_error_code: 'ctox_file_chunk_integrity_mismatch',
      file_integrity_error_phase: 'file-chunk-reconstruct',
      requested_generation_chunk_count: 0,
    },
  },
  'workspace-rust-to-browser': { keys: ['replicated_id'] },
  'workspace-agent-artifacts-rust-to-browser': {
    keys: ['replicated_count', 'replicated_ids', 'virtual_paths', 'payload_lengths', 'chunk_counts', 'total_chunk_count', 'max_chunk_count'],
    values: { replicated_count: 4 },
  },
  'workspace-agent-artifacts-stress-rust-to-browser': {
    keys: ['replicated_count', 'replicated_ids', 'virtual_paths', 'payload_lengths', 'chunk_counts', 'total_chunk_count', 'max_chunk_count'],
    values: { replicated_count: 16 },
    minimums: { total_chunk_count: 20, max_chunk_count: 2 },
  },
  'workspace-agent-artifacts-churn-rust-to-browser': {
    keys: [
      'replicated_count',
      'replicated_ids',
      'virtual_paths',
      'payload_lengths',
      'chunk_counts',
      'total_chunk_count',
      'max_chunk_count',
      'updated_generation_changes',
      'added_count',
      'updated_relative_paths',
      'added_relative_paths',
    ],
    values: { replicated_count: 20, updated_generation_changes: 4, added_count: 4 },
    minimums: { total_chunk_count: 20, max_chunk_count: 2 },
  },
  ...businessOsProductionSmokeEvidenceRequirements,
  'workspace-agent-artifacts-background-rust-to-browser': {
    keys: [
      'replicated_count',
      'replicated_ids',
      'virtual_paths',
      'payload_lengths',
      'chunk_counts',
      'total_chunk_count',
      'max_chunk_count',
      'background_indexer',
      'background_queue_task_created',
      'background_queue_task_id',
    ],
    values: { replicated_count: 4, background_indexer: 1, background_queue_task_created: 1 },
  },
  'workspace-update-rust-to-browser': {
    keys: ['replicated_id', 'previous_generation', 'updated_generation'],
  },
  'workspace-large-materialize-rust-to-browser': {
    keys: ['replicated_id', 'generation', 'chunk_count', 'payload_length'],
  },
  'workspace-large-file-viewer-rust-to-browser': {
    keys: ['replicated_id', 'generation', 'chunk_count', 'payload_length'],
  },
  'workspace-large-file-viewer-restart-rust-to-browser': {
    keys: ['advanced_status', 'replicated_id', 'generation', 'chunk_count', 'payload_length'],
    values: { advanced_status: 'business-os-advanced-status-v1' },
  },
};
const modes = (process.env.SMOKE_MODES || defaultModes.join(','))
  .split(/[,\s]+/)
  .map((mode) => mode.trim())
  .filter(Boolean);
const knownModes = new Set([...defaultModes, ...Object.keys(modeEvidenceRequirements)]);
const pagePath = process.env.SMOKE_PAGE_PATH || '/index.html';
const businessPortBaseInput = process.env.BUSINESS_PORT || '8877';
const signalingPortBaseInput = process.env.SIGNALING_PORT || '18876';
const attemptsInput = process.env.SMOKE_MATRIX_ATTEMPTS || '2';
const modeTimeoutInput = process.env.SMOKE_MODE_TIMEOUT_MS || '180000';
const websocketWarningBudgetInput = process.env.SMOKE_BROWSER_WEBSOCKET_WARNING_BUDGET || '5';
const startupReloadBudgetInput = process.env.SMOKE_STARTUP_RELOAD_BUDGET || '0';
const startupHookWaitBudgetInput = process.env.SMOKE_STARTUP_HOOK_WAIT_BUDGET_MS || '60000';
const fileChunkStatusStartupHookWaitBudgetInput = process.env.SMOKE_FILE_CHUNK_STATUS_STARTUP_HOOK_WAIT_BUDGET_MS || '12000';
const browserWarningBudgetInput = process.env.SMOKE_BROWSER_WARNING_BUDGET;
const browserErrorBudgetInput = process.env.SMOKE_BROWSER_ERROR_BUDGET;
const requestFailureBudgetInput = process.env.SMOKE_BROWSER_REQUEST_FAILURE_BUDGET;
const assetResponseErrorBudgetInput = process.env.SMOKE_BROWSER_ASSET_RESPONSE_ERROR_BUDGET || '0';
const networkFlapBrowserWarningBudgetInput = process.env.SMOKE_NETWORK_FLAP_BROWSER_WARNING_BUDGET;
const networkFlapRequestFailureBudgetInput = process.env.SMOKE_NETWORK_FLAP_REQUEST_FAILURE_BUDGET;
const modeDurationBudgetInput = process.env.SMOKE_MODE_DURATION_BUDGET_MS;
const syncConfigWaitBudgetInput = process.env.SMOKE_SYNC_CONFIG_WAIT_BUDGET_MS;
const resultPath = process.env.SMOKE_MATRIX_RESULT_PATH || defaultResultPath;
const requireEvidence = process.env.SMOKE_REQUIRE_EVIDENCE !== '0';
const summary = {
  schema: SMOKE_MATRIX_SUMMARY_SCHEMA,
  schemaVersion: 1,
  repositoryRoot: root,
  gitRevision: readGitRevision(),
  source: sourceEvidence(),
  ctoxBin,
  resultPath,
  pagePath,
  requireEvidence,
  requestedModes: modes,
  modes: [],
  startedAt: new Date().toISOString(),
  endedAt: null,
  ok: false,
};
if (process.env.SMOKE_MATRIX_SELF_TEST === '1' || process.argv.includes('--self-test')) {
  runSmokeMatrixSelfTest();
  process.exit(0);
}
const businessPortBase = parsePositiveIntegerConfig('BUSINESS_PORT', businessPortBaseInput, { max: 65535 });
const signalingPortBase = parsePositiveIntegerConfig('SIGNALING_PORT', signalingPortBaseInput, { max: 65535 });
const attempts = parsePositiveIntegerConfig('SMOKE_MATRIX_ATTEMPTS', attemptsInput, { max: 20 });
const modeTimeoutMs = parsePositiveIntegerConfig('SMOKE_MODE_TIMEOUT_MS', modeTimeoutInput, { max: 60 * 60 * 1000 });
const websocketWarningBudget = parseNonNegativeIntegerConfig(
  'SMOKE_BROWSER_WEBSOCKET_WARNING_BUDGET',
  websocketWarningBudgetInput,
  { max: 1000 },
);
const startupReloadBudget = parseNonNegativeIntegerConfig(
  'SMOKE_STARTUP_RELOAD_BUDGET',
  startupReloadBudgetInput,
  { max: 10 },
);
const startupHookWaitBudgetMs = parseNonNegativeIntegerConfig(
  'SMOKE_STARTUP_HOOK_WAIT_BUDGET_MS',
  startupHookWaitBudgetInput,
  { max: 60 * 60 * 1000 },
);
const fileChunkStatusStartupHookWaitBudgetMs = parseNonNegativeIntegerConfig(
  'SMOKE_FILE_CHUNK_STATUS_STARTUP_HOOK_WAIT_BUDGET_MS',
  fileChunkStatusStartupHookWaitBudgetInput,
  { max: 60 * 60 * 1000 },
);
const browserWarningBudget = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_BROWSER_WARNING_BUDGET',
  browserWarningBudgetInput,
  { max: 100000 },
);
const browserErrorBudget = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_BROWSER_ERROR_BUDGET',
  browserErrorBudgetInput,
  { max: 100000 },
);
const requestFailureBudget = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_BROWSER_REQUEST_FAILURE_BUDGET',
  requestFailureBudgetInput,
  { max: 100000 },
);
const assetResponseErrorBudget = parseNonNegativeIntegerConfig(
  'SMOKE_BROWSER_ASSET_RESPONSE_ERROR_BUDGET',
  assetResponseErrorBudgetInput,
  { max: 100000 },
);
const networkFlapBrowserWarningBudget = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_NETWORK_FLAP_BROWSER_WARNING_BUDGET',
  networkFlapBrowserWarningBudgetInput,
  { max: 100000 },
);
const networkFlapRequestFailureBudget = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_NETWORK_FLAP_REQUEST_FAILURE_BUDGET',
  networkFlapRequestFailureBudgetInput,
  { max: 100000 },
);
const modeDurationBudgetMs = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_MODE_DURATION_BUDGET_MS',
  modeDurationBudgetInput,
  { max: 60 * 60 * 1000 },
);
const syncConfigWaitBudgetMs = parseOptionalNonNegativeIntegerConfig(
  'SMOKE_SYNC_CONFIG_WAIT_BUDGET_MS',
  syncConfigWaitBudgetInput,
  { max: 60 * 60 * 1000 },
);
summary.configuration = {
  attempts,
  modeTimeoutMs,
  businessPortBase,
  signalingPortBase,
  budgets: {
    browserWarningBudget,
    browserErrorBudget,
    websocketWarningBudget,
    requestFailureBudget,
    assetResponseErrorBudget,
    startupReloadBudget,
    startupHookWaitBudgetMs,
    fileChunkStatusStartupHookWaitBudgetMs,
    modeDurationBudgetMs,
    syncConfigWaitBudgetMs,
  },
};

const unknownModes = modes.filter((mode) => !knownModes.has(mode));
if (!modes.length) {
  failConfiguration('SMOKE_MODES did not contain any smoke modes');
}
if (unknownModes.length) {
  failConfiguration(`SMOKE_MODES contains unsupported mode(s): ${unknownModes.join(', ')}`);
}
const maxPortOffset = (modes.length * attempts) - 1;
if (businessPortBase + maxPortOffset > 65535) {
  failConfiguration(`BUSINESS_PORT plus matrix port range exceeds 65535: ${businessPortBase}+${maxPortOffset}`);
}
if (signalingPortBase + maxPortOffset > 65535) {
  failConfiguration(`SIGNALING_PORT plus matrix port range exceeds 65535: ${signalingPortBase}+${maxPortOffset}`);
}

ensureCtoxSmokeBinary();
// The summary object is created before configuration/build validation so that
// configuration failures can still emit a diagnostic artifact. On a clean CI
// runner the smoke binary does not exist at that point and its initial hash is
// therefore null. Refresh source evidence after the build before validating or
// executing the matrix.
summary.source = sourceEvidence();

for (const [index, mode] of modes.entries()) {
  const effectiveBrowserWarningBudget = mode === 'network-flap-browser-to-rust' && networkFlapBrowserWarningBudget !== null
    ? networkFlapBrowserWarningBudget
    : browserWarningBudget;
  const effectiveRequestFailureBudget = mode === 'network-flap-browser-to-rust' && networkFlapRequestFailureBudget !== null
    ? networkFlapRequestFailureBudget
    : requestFailureBudget;
  const effectiveStartupHookWaitBudgetMs = isFileChunkStatusMode(mode)
    ? Math.min(startupHookWaitBudgetMs, fileChunkStatusStartupHookWaitBudgetMs)
    : startupHookWaitBudgetMs;
  let lastStatus = 1;
  let lastSignal = null;
  const modeSummary = {
    mode,
    attempts: [],
    ok: false,
  };
  summary.modes.push(modeSummary);
  writeSummary(false);
  for (let attempt = 1; attempt <= attempts; attempt++) {
    const attemptStartedAt = Date.now();
    const portOffset = index * attempts + (attempt - 1);
    const effectivePagePath = ['browser-lifecycle-ui', 'browser-handoff-ui'].includes(mode)
      && pagePath === '/index.html'
      ? '/index.html#browser'
      : pagePath;
    const env = {
      ...process.env,
      SMOKE_PAGE_PATH: effectivePagePath,
      SMOKE_MODE: mode,
      BUSINESS_PORT: String(businessPortBase + portOffset),
      SIGNALING_PORT: String(signalingPortBase + portOffset),
      CTOX_SKIP_SMOKE_BUILD: '1',
    };
    console.log(`\n=== rxdb smoke: ${mode} (${effectivePagePath}) attempt ${attempt}/${attempts} ===`);
    const attemptLogPrefix = path.join(
      os.tmpdir(),
      `ctox-rxdb-smoke-${process.pid}-${index}-${attempt}-${Date.now()}`
    );
    const stdoutPath = `${attemptLogPrefix}.stdout.log`;
    const stderrPath = `${attemptLogPrefix}.stderr.log`;
    const processLifecyclePath = `${attemptLogPrefix}.process-lifecycle.json`;
    const attemptRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-rxdb-smoke-'));
    env.CTOX_SMOKE_ROOT = attemptRoot;
    env.CTOX_SMOKE_RUN_ID = `matrix-${process.pid}-${index}-${attempt}-${Date.now()}`;
    env.SMOKE_PROCESS_LIFECYCLE_PATH = processLifecyclePath;
    const stdoutFd = fs.openSync(stdoutPath, 'w');
    const stderrFd = fs.openSync(stderrPath, 'w');
    let result;
    try {
      result = spawnSync(process.execPath, [toolPath], {
        cwd: root,
        env,
        encoding: 'utf8',
        stdio: ['ignore', stdoutFd, stderrFd],
        timeout: modeTimeoutMs,
        killSignal: 'SIGTERM',
      });
    } finally {
      fs.closeSync(stdoutFd);
      fs.closeSync(stderrFd);
    }
    const durationMs = Date.now() - attemptStartedAt;
    const stdout = readTextFile(stdoutPath);
    const stderr = readTextFile(stderrPath);
    const processLifecycle = readJsonFile(processLifecyclePath);
    try { fs.unlinkSync(stdoutPath); } catch {}
    try { fs.unlinkSync(stderrPath); } catch {}
    try { fs.unlinkSync(processLifecyclePath); } catch {}
    if (process.env.CTOX_SMOKE_KEEP_ARTIFACTS !== '1') removeTempPath(attemptRoot);
    if (stdout) process.stdout.write(stdout);
    if (stderr) process.stderr.write(stderr);
    const evidence = parseSmokeEvidence(`${stdout}\n${stderr}`);
    const evidenceProblems = requireEvidence && result.status === 0 && !result.signal
      ? validateModeEvidence(mode, evidence)
      : [];
    const browserDiagnosticsProblems = result.status === 0 && !result.signal
      ? validateBrowserDiagnosticsBudget(evidence, {
          websocketWarningBudget,
          browserWarningBudget: effectiveBrowserWarningBudget,
          browserErrorBudget,
          requestFailureBudget: effectiveRequestFailureBudget,
          assetResponseErrorBudget,
          syncConfigWaitBudgetMs,
        })
      : [];
    const startupBudgetProblems = result.status === 0 && !result.signal
      ? validateStartupBudget(evidence, {
          startupReloadBudget,
          startupHookWaitBudgetMs: effectiveStartupHookWaitBudgetMs,
        })
      : [];
    const durationBudgetProblems = validateDurationBudget(durationMs, modeDurationBudgetMs);
    evidenceProblems.push(...browserDiagnosticsProblems, ...startupBudgetProblems, ...durationBudgetProblems);
    if (evidenceProblems.length) {
      console.error(`smoke ${mode} missing required evidence: ${evidenceProblems.join(', ')}`);
    }
    lastStatus = evidenceProblems.length ? 1 : (result.status || 0);
    lastSignal = result.signal || null;
    const timedOut = result.error?.code === 'ETIMEDOUT';
    if (timedOut) {
      console.error(`smoke ${mode} timed out after ${modeTimeoutMs} ms`);
      lastStatus = 1;
    }
    modeSummary.attempts.push({
      attempt,
      status: result.status,
      signal: result.signal || null,
      timedOut,
      timeoutMs: modeTimeoutMs,
      pagePath: effectivePagePath,
      url: `http://127.0.0.1:${env.BUSINESS_PORT}${effectivePagePath}`,
      businessPort: Number(env.BUSINESS_PORT),
      signalingPort: Number(env.SIGNALING_PORT),
      durationMs,
      ok: lastStatus === 0 && !lastSignal,
      error: result.error ? { code: result.error.code || '', message: result.error.message || '' } : null,
      failureOutput: lastStatus !== 0 || lastSignal
        ? {
            stdoutTail: logTail(stdout),
            stderrTail: logTail(stderr),
          }
        : null,
      processLifecycle,
      context: smokeAttemptContextFromEvidence(evidence),
      evidenceKeys: Object.keys(evidence).sort(),
      evidence,
      warningBudget: {
        browserWarnings: Number(evidence.browser_warning_count || 0),
        maxBrowserWarnings: effectiveBrowserWarningBudget,
        browserErrors: Number(evidence.browser_error_count || 0),
        maxBrowserErrors: browserErrorBudget,
        websocketWarnings: Number(evidence.browser_websocket_warning_count || 0),
        maxWebsocketWarnings: websocketWarningBudget,
        requestFailures: Number(evidence.browser_request_failure_count || 0),
        maxRequestFailures: effectiveRequestFailureBudget,
        assetResponseErrors: Number(evidence.browser_asset_response_error_count || 0),
        maxAssetResponseErrors: assetResponseErrorBudget,
        startupReloads: Number(evidence.startup_smoke_hook_reload_count || 0),
        maxStartupReloads: startupReloadBudget,
        startupHookWaitMs: Number(evidence.startup_smoke_hook_wait_ms || 0),
        maxStartupHookWaitMs: effectiveStartupHookWaitBudgetMs,
        syncConfigWaitMs: Number(evidence.ctox_sync_config_wait_ms || 0),
        maxSyncConfigWaitMs: syncConfigWaitBudgetMs,
        durationMs,
        maxDurationMs: modeDurationBudgetMs,
      },
      evidenceProblems,
    });
    writeSummary(false);
    const outcome = lastStatus === 0 && !lastSignal ? 'OK' : 'FAILED';
    console.log(`=== rxdb smoke result: ${mode} attempt ${attempt}/${attempts} ${outcome} durationMs=${durationMs} timedOut=${timedOut} ===`);
    if (lastStatus === 0 && !lastSignal) break;
    if (attempt < attempts) {
      console.error(`smoke ${mode} failed; retrying once`);
    }
  }
  modeSummary.ok = lastStatus === 0 && !lastSignal;
  writeSummary(false);
  if (lastSignal) {
    console.error(`smoke ${mode} terminated by signal ${lastSignal}`);
    writeSummary(false);
    process.exit(1);
  }
  if (lastStatus !== 0) {
    writeSummary(false);
    process.exit(lastStatus || 1);
  }
}

writeSummary(true);
console.log(`\nrxdb smoke matrix OK: ${modes.join(', ')}`);

function runSmokeMatrixSelfTest() {
  const runnerSource = fs.readFileSync(toolPath, 'utf8');
  const runnerUsesSharedModeList = runnerSource.includes('...businessOsProductionSmokeModes');
  const runnerBlocksUnimplementedModes = runnerSource.includes('businessOsProductionSmokeModeSet.has(smokeMode)');
  if (!runnerUsesSharedModeList || !runnerBlocksUnimplementedModes) {
    throw new Error('Business OS production smoke modes are not wired into browser_rust_smoke.js');
  }
  if (!runnerSource.includes("&& smokeMode !== 'business-os-app-audience-ui'")) {
    throw new Error('App audience policy smoke must remain independent of deferred file replication');
  }
  const matrixSource = fs.readFileSync(__filename, 'utf8');
  const ensureBinaryIndex = matrixSource.indexOf('\nensureCtoxSmokeBinary();\n');
  const refreshSourceIndex = matrixSource.indexOf('\nsummary.source = sourceEvidence();\n', ensureBinaryIndex);
  if (ensureBinaryIndex < 0 || refreshSourceIndex < ensureBinaryIndex) {
    throw new Error('Smoke matrix source evidence is not refreshed after building the smoke binary');
  }
  const expectedMatrixSourceSha256 = crypto.createHash('sha256').update(matrixSource).digest('hex');
  if (sha256File(__filename) !== expectedMatrixSourceSha256) {
    throw new Error('Smoke matrix chunked SHA-256 helper does not match the in-memory digest');
  }
  assertBusinessOsProductionSmokeRegistry({
    runnerModes: businessOsProductionSmokeModes,
    matrixModes: knownModes,
    modeEvidenceRequirements,
  });
  assertSelfTestThrows('missing runner mode', () => {
    assertBusinessOsProductionSmokeRegistry({
      runnerModes: businessOsProductionSmokeModes.slice(1),
      matrixModes: knownModes,
      modeEvidenceRequirements,
    });
  });
  assertSelfTestThrows('missing matrix mode', () => {
    assertBusinessOsProductionSmokeRegistry({
      runnerModes: businessOsProductionSmokeModes,
      matrixModes: businessOsProductionSmokeModes.slice(1),
      modeEvidenceRequirements,
    });
  });
  assertSelfTestThrows('missing evidence requirement', () => {
    const brokenRequirements = { ...modeEvidenceRequirements };
    delete brokenRequirements[businessOsProductionSmokeModes[0]];
    assertBusinessOsProductionSmokeRegistry({
      runnerModes: businessOsProductionSmokeModes,
      matrixModes: knownModes,
      modeEvidenceRequirements: brokenRequirements,
    });
  });
  for (const mode of businessOsProductionSmokeModes) {
    const missingEvidence = validateModeEvidence(mode, {});
    const required = evidenceRequirementsForMode(mode);
    for (const key of required.keys) {
      if (!missingEvidence.includes(key)) {
        throw new Error(`Missing-evidence self-test did not require ${mode}:${key}`);
      }
    }
  }
  assertSelfTestThrows('unsupported evidence mode', () => evidenceRequirementsForMode('__missing_business_os_smoke__'));
  const validSummary = makeSmokeMatrixSummarySelfTestArtifact();
  const validProblems = validateSmokeMatrixSummaryArtifact(validSummary, { final: true });
  if (validProblems.length) {
    throw new Error(`Valid smoke summary fixture failed schema validation: ${validProblems.join(', ')}`);
  }
  const configurationFailureSummary = JSON.parse(JSON.stringify(validSummary));
  configurationFailureSummary.ok = false;
  configurationFailureSummary.configurationError = 'missing smoke binary';
  configurationFailureSummary.configuration = null;
  configurationFailureSummary.modes = [];
  configurationFailureSummary.source.artifactHashes.smokeBinarySha256 = null;
  const configurationFailureProblems = validateSmokeMatrixSummaryArtifact(
    configurationFailureSummary,
    { final: false },
  );
  if (configurationFailureProblems.length) {
    throw new Error(`Configuration-failure summary lost its diagnostic artifact: ${configurationFailureProblems.join(', ')}`);
  }
  assertSelfTestThrows('configuration failure accepted as final', () => {
    validateSmokeMatrixSummaryArtifact(
      configurationFailureSummary,
      { final: true },
      { throwOnError: true },
    );
  });
  assertSelfTestThrows('missing smoke artifact git revision', () => {
    validateSmokeMatrixSummaryArtifact({ ...validSummary, gitRevision: '' }, { final: true }, { throwOnError: true });
  });
  assertSelfTestThrows('missing smoke artifact attempt URL', () => {
    const broken = JSON.parse(JSON.stringify(validSummary));
    delete broken.modes[0].attempts[0].url;
    validateSmokeMatrixSummaryArtifact(broken, { final: true }, { throwOnError: true });
  });
  assertSelfTestThrows('missing smoke artifact configuration', () => {
    const broken = JSON.parse(JSON.stringify(validSummary));
    delete broken.configuration;
    validateSmokeMatrixSummaryArtifact(broken, { final: true }, { throwOnError: true });
  });
  assertSelfTestThrows('smoke artifact evidence key drift', () => {
    const broken = JSON.parse(JSON.stringify(validSummary));
    broken.modes[0].attempts[0].evidenceKeys = ['advanced_status'];
    validateSmokeMatrixSummaryArtifact(broken, { final: true }, { throwOnError: true });
  });
  assertSelfTestThrows('smoke artifact production context drift', () => {
    const broken = JSON.parse(JSON.stringify(validSummary));
    broken.modes[0].attempts[0].context.tenantScope = '';
    validateSmokeMatrixSummaryArtifact(broken, { final: true }, { throwOnError: true });
  });
  assertSelfTestThrows('smoke artifact production warning budget drift', () => {
    const broken = JSON.parse(JSON.stringify(validSummary));
    broken.modes[0].attempts[0].warningBudget.browserWarnings = 1;
    validateSmokeMatrixSummaryArtifact(broken, { final: true }, { throwOnError: true });
  });
  assertSelfTestThrows('smoke artifact production browser error budget drift', () => {
    const broken = JSON.parse(JSON.stringify(validSummary));
    broken.modes[0].attempts[0].warningBudget.browserErrors = 1;
    validateSmokeMatrixSummaryArtifact(broken, { final: true }, { throwOnError: true });
  });
  const browserErrorBudgetProblems = validateBrowserDiagnosticsBudget(
    { browser_error_count: 1, browser_websocket_warning_count: 0, browser_asset_response_error_count: 0 },
    {
      websocketWarningBudget: 5,
      browserWarningBudget: null,
      browserErrorBudget: 0,
      requestFailureBudget: null,
      assetResponseErrorBudget: 0,
      syncConfigWaitBudgetMs: null,
    },
  );
  if (!browserErrorBudgetProblems.includes('browser_error_count<=0')) {
    throw new Error('Browser-error budget self-test did not reject browser_error_count=1');
  }
  if (logTail('short diagnostic') !== 'short diagnostic') {
    throw new Error('Failure-output log tail changed short diagnostics');
  }
  const longDiagnostic = `${'x'.repeat(20 * 1024)}diagnostic-suffix`;
  const truncatedDiagnostic = logTail(longDiagnostic);
  if (!truncatedDiagnostic.includes('diagnostic-suffix')
    || Buffer.byteLength(truncatedDiagnostic.split('\n').slice(1).join('\n'), 'utf8') > 16 * 1024) {
    throw new Error('Failure-output log tail did not retain a bounded diagnostic suffix');
  }
  console.log(`business_os_production_smoke_registry_modes=${businessOsProductionSmokeModes.join(',')}`);
  console.log('business_os_production_smoke_registry_self_test=1');
}

function assertSelfTestThrows(label, fn) {
  try {
    fn();
  } catch {
    return;
  }
  throw new Error(`Smoke matrix self-test expected failure for ${label}`);
}

function writeSummary(ok) {
  summary.ok = ok;
  summary.endedAt = new Date().toISOString();
  const schemaProblems = validateSmokeMatrixSummaryArtifact(summary, { final: ok });
  if (schemaProblems.length) {
    throw new Error(`Smoke matrix summary artifact failed schema validation: ${schemaProblems.join(', ')}`);
  }
  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, `${JSON.stringify(summary, null, 2)}\n`);
}

function readGitRevision() {
  try {
    const result = spawnSync('git', ['-C', root, 'rev-parse', 'HEAD'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const revision = String(result.stdout || '').trim();
    return result.status === 0 && revision ? revision : 'unknown';
  } catch {
    return 'unknown';
  }
}

function sourceEvidence() {
  return {
    commit: readGitRevision(),
    dirty: Boolean(readGitStatusPorcelain()),
    artifactHashes: {
      browserBundleSha256: sha256File(path.join(root, 'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs')),
      smokeBinaryPath: ctoxBin,
      smokeBinarySha256: sha256File(ctoxBin),
    },
  };
}

function readGitStatusPorcelain() {
  try {
    const result = spawnSync('git', ['-C', root, 'status', '--porcelain=v1', '--untracked-files=all'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    return result.status === 0 ? String(result.stdout || '').trim() : '';
  } catch {
    return '';
  }
}

function sha256File(filePath) {
  let descriptor;
  try {
    descriptor = fs.openSync(filePath, 'r');
    const hash = crypto.createHash('sha256');
    const buffer = Buffer.allocUnsafe(4 * 1024 * 1024);
    while (true) {
      const bytesRead = fs.readSync(descriptor, buffer, 0, buffer.length, null);
      if (bytesRead === 0) break;
      hash.update(buffer.subarray(0, bytesRead));
    }
    return hash.digest('hex');
  } catch {
    return null;
  } finally {
    if (descriptor !== undefined) fs.closeSync(descriptor);
  }
}

function readJsonFile(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {
    return null;
  }
}

function smokeAttemptContextFromEvidence(evidence) {
  const keyValue = (suffix) => {
    const key = Object.keys(evidence || {}).find((candidate) => candidate.endsWith(suffix));
    return key ? evidence[key] : null;
  };
  return {
    authState: keyValue('_auth_state'),
    actorRole: keyValue('_actor_role'),
    browserContext: keyValue('_browser_context'),
    tenantScope: keyValue('_tenant_scope'),
    advancedStatus: evidence?.advanced_status || null,
  };
}

function validateSmokeMatrixSummaryArtifact(candidate, options = {}, validationOptions = {}) {
  const problems = [];
  const require = (condition, message) => {
    if (!condition) problems.push(message);
  };
  require(candidate && typeof candidate === 'object', 'summary_object');
  if (!candidate || typeof candidate !== 'object') return finishSummaryValidation(problems, validationOptions);
  const configurationFailed = typeof candidate.configurationError === 'string'
    && candidate.configurationError.length > 0;
  require(candidate.schema === SMOKE_MATRIX_SUMMARY_SCHEMA, 'schema');
  require(candidate.schemaVersion === 1, 'schemaVersion');
  require(typeof candidate.repositoryRoot === 'string' && candidate.repositoryRoot.length > 0, 'repositoryRoot');
  require(typeof candidate.gitRevision === 'string' && candidate.gitRevision.length >= 7, 'gitRevision');
  require(candidate.source && typeof candidate.source === 'object', 'source');
  if (candidate.source && typeof candidate.source === 'object') {
    require(typeof candidate.source.commit === 'string' && candidate.source.commit.length >= 7, 'source.commit');
    require(typeof candidate.source.dirty === 'boolean', 'source.dirty');
    require(candidate.source.artifactHashes && typeof candidate.source.artifactHashes === 'object', 'source.artifactHashes');
    if (candidate.source.artifactHashes && typeof candidate.source.artifactHashes === 'object') {
      require(isSha256(candidate.source.artifactHashes.browserBundleSha256), 'source.artifactHashes.browserBundleSha256');
      require(typeof candidate.source.artifactHashes.smokeBinaryPath === 'string' && candidate.source.artifactHashes.smokeBinaryPath.length > 0, 'source.artifactHashes.smokeBinaryPath');
      if (!configurationFailed || options.final) {
        require(isSha256(candidate.source.artifactHashes.smokeBinarySha256), 'source.artifactHashes.smokeBinarySha256');
      }
    }
  }
  require(typeof candidate.ctoxBin === 'string' && candidate.ctoxBin.length > 0, 'ctoxBin');
  require(typeof candidate.resultPath === 'string' && candidate.resultPath.length > 0, 'resultPath');
  require(typeof candidate.pagePath === 'string' && candidate.pagePath.startsWith('/'), 'pagePath');
  require(typeof candidate.requireEvidence === 'boolean', 'requireEvidence');
  require(Array.isArray(candidate.requestedModes) && candidate.requestedModes.length > 0, 'requestedModes');
  require(Array.isArray(candidate.modes), 'modes');
  if (!configurationFailed || options.final) {
    require(candidate.configuration && typeof candidate.configuration === 'object', 'configuration');
  }
  if (candidate.configuration && typeof candidate.configuration === 'object') {
    const config = candidate.configuration;
    require(Number.isInteger(config.attempts) && config.attempts > 0, 'configuration.attempts');
    require(Number.isInteger(config.modeTimeoutMs) && config.modeTimeoutMs > 0, 'configuration.modeTimeoutMs');
    require(Number.isInteger(config.businessPortBase) && config.businessPortBase > 0, 'configuration.businessPortBase');
    require(Number.isInteger(config.signalingPortBase) && config.signalingPortBase > 0, 'configuration.signalingPortBase');
    require(config.budgets && typeof config.budgets === 'object', 'configuration.budgets');
  }
  require(typeof candidate.startedAt === 'string' && candidate.startedAt.length > 0, 'startedAt');
  require(typeof candidate.endedAt === 'string' && candidate.endedAt.length > 0, 'endedAt');
  require(typeof candidate.ok === 'boolean', 'ok');
  if (options.final) {
    require(!configurationFailed, 'configurationError_absent');
    require(candidate.ok === true, 'ok=true');
    require(candidate.modes.length === candidate.requestedModes.length, 'modes_complete');
    const completedModes = new Set(candidate.modes.map((mode) => mode?.mode));
    for (const requestedMode of candidate.requestedModes) {
      require(completedModes.has(requestedMode), `requestedModes.${requestedMode}`);
    }
  }
  for (const [modeIndex, mode] of (candidate.modes || []).entries()) {
    const prefix = `modes[${modeIndex}]`;
    require(typeof mode.mode === 'string' && mode.mode.length > 0, `${prefix}.mode`);
    require(Array.isArray(mode.attempts), `${prefix}.attempts`);
    require(typeof mode.ok === 'boolean', `${prefix}.ok`);
    if (options.final) require(mode.ok === true, `${prefix}.ok=true`);
    if (options.final) {
      require((mode.attempts || []).some((attempt) => attempt?.ok === true), `${prefix}.accepted_attempt`);
    }
    for (const [attemptIndex, attempt] of (mode.attempts || []).entries()) {
      const attemptPrefix = `${prefix}.attempts[${attemptIndex}]`;
      require(Number.isInteger(attempt.attempt) && attempt.attempt >= 1, `${attemptPrefix}.attempt`);
      require(typeof attempt.url === 'string' && attempt.url.startsWith('http://127.0.0.1:'), `${attemptPrefix}.url`);
      require(typeof attempt.pagePath === 'string' && attempt.pagePath.startsWith('/'), `${attemptPrefix}.pagePath`);
      require(Number.isInteger(attempt.businessPort) && attempt.businessPort > 0, `${attemptPrefix}.businessPort`);
      require(Number.isInteger(attempt.signalingPort) && attempt.signalingPort > 0, `${attemptPrefix}.signalingPort`);
      require(Number.isFinite(attempt.durationMs) && attempt.durationMs >= 0, `${attemptPrefix}.durationMs`);
      require(typeof attempt.ok === 'boolean', `${attemptPrefix}.ok`);
      require(attempt.processLifecycle && typeof attempt.processLifecycle === 'object', `${attemptPrefix}.processLifecycle`);
      if (attempt.processLifecycle && typeof attempt.processLifecycle === 'object') {
        const lifecycle = attempt.processLifecycle;
        require(lifecycle.schema === 'ctox.rxdb.smoke_process_lifecycle.v1', `${attemptPrefix}.processLifecycle.schema`);
        require(typeof lifecycle.runId === 'string' && lifecycle.runId.length > 0, `${attemptPrefix}.processLifecycle.runId`);
        require(Number.isInteger(lifecycle.parent?.pid) && lifecycle.parent.pid > 0, `${attemptPrefix}.processLifecycle.parent.pid`);
        require(Number.isInteger(lifecycle.parent?.ppid) && lifecycle.parent.ppid > 0, `${attemptPrefix}.processLifecycle.parent.ppid`);
        require(Array.isArray(lifecycle.events) && lifecycle.events.length > 0, `${attemptPrefix}.processLifecycle.events`);
        if (options.final && attempt.ok === true && Array.isArray(lifecycle.events)) {
          const unknownSignal = lifecycle.events.some((event) => (
            event?.type === 'child_exited'
            && event?.signal
            && event?.signalSource === 'unknown_external_source'
          ));
          require(!unknownSignal, `${attemptPrefix}.processLifecycle.unknown_child_signal`);
          require(
            lifecycle.events.some((event) => event?.type === 'smoke_cleanup_complete'),
            `${attemptPrefix}.processLifecycle.cleanup_complete`,
          );
        }
      }
      require(attempt.evidence && typeof attempt.evidence === 'object', `${attemptPrefix}.evidence`);
      require(Array.isArray(attempt.evidenceKeys), `${attemptPrefix}.evidenceKeys`);
      require(attempt.warningBudget && typeof attempt.warningBudget === 'object', `${attemptPrefix}.warningBudget`);
      require(attempt.context && typeof attempt.context === 'object', `${attemptPrefix}.context`);
      if (attempt.evidence && typeof attempt.evidence === 'object' && Array.isArray(attempt.evidenceKeys)) {
        const evidenceKeys = Object.keys(attempt.evidence).sort();
        require(
          JSON.stringify(attempt.evidenceKeys) === JSON.stringify(evidenceKeys),
          `${attemptPrefix}.evidenceKeys_match_evidence`,
        );
      }
      if (attempt.warningBudget && typeof attempt.warningBudget === 'object') {
        const numericBudgetKeys = [
          'browserWarnings',
          'browserErrors',
          'websocketWarnings',
          'requestFailures',
          'assetResponseErrors',
          'startupReloads',
          'startupHookWaitMs',
          'syncConfigWaitMs',
          'durationMs',
        ];
        const nullableBudgetKeys = [
          'maxBrowserWarnings',
          'maxBrowserErrors',
          'maxWebsocketWarnings',
          'maxRequestFailures',
          'maxAssetResponseErrors',
          'maxStartupReloads',
          'maxStartupHookWaitMs',
          'maxSyncConfigWaitMs',
          'maxDurationMs',
        ];
        for (const key of numericBudgetKeys) {
          require(Number.isFinite(attempt.warningBudget[key]) && attempt.warningBudget[key] >= 0, `${attemptPrefix}.warningBudget.${key}`);
        }
        for (const key of nullableBudgetKeys) {
          require(
            attempt.warningBudget[key] === null
              || (Number.isFinite(attempt.warningBudget[key]) && attempt.warningBudget[key] >= 0),
            `${attemptPrefix}.warningBudget.${key}`,
          );
        }
      }
      if (attempt.context && typeof attempt.context === 'object') {
        for (const key of ['authState', 'actorRole', 'browserContext', 'tenantScope', 'advancedStatus']) {
          require(Object.prototype.hasOwnProperty.call(attempt.context, key), `${attemptPrefix}.context.${key}`);
        }
      }
      if (options.final && attempt.ok === true && businessOsProductionSmokeModeSet.has(mode.mode)) {
        requireProductionAttemptContext(attempt.context, attemptPrefix, require);
        requireSuccessfulAttemptWithinBudgets(attempt.warningBudget, attemptPrefix, require);
      }
    }
  }
  return finishSummaryValidation(problems, validationOptions);
}

function isSha256(value) {
  return typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value);
}

function requireProductionAttemptContext(context, attemptPrefix, require) {
  if (!context || typeof context !== 'object') return;
  for (const key of ['authState', 'actorRole', 'browserContext', 'tenantScope']) {
    require(typeof context[key] === 'string' && context[key].length > 0, `${attemptPrefix}.context.${key}.non_empty`);
  }
  require(
    context.advancedStatus === 'business-os-advanced-status-v1',
    `${attemptPrefix}.context.advancedStatus.business_os_v1`,
  );
}

function requireSuccessfulAttemptWithinBudgets(warningBudget, attemptPrefix, require) {
  if (!warningBudget || typeof warningBudget !== 'object') return;
  const checks = [
    ['browserWarnings', 'maxBrowserWarnings'],
    ['browserErrors', 'maxBrowserErrors'],
    ['websocketWarnings', 'maxWebsocketWarnings'],
    ['requestFailures', 'maxRequestFailures'],
    ['assetResponseErrors', 'maxAssetResponseErrors'],
    ['startupReloads', 'maxStartupReloads'],
    ['startupHookWaitMs', 'maxStartupHookWaitMs'],
    ['syncConfigWaitMs', 'maxSyncConfigWaitMs'],
    ['durationMs', 'maxDurationMs'],
  ];
  for (const [actualKey, maxKey] of checks) {
    const actual = warningBudget[actualKey];
    const max = warningBudget[maxKey];
    if (max === null) continue;
    if (!Number.isFinite(actual) || !Number.isFinite(max)) continue;
    require(actual <= max, `${attemptPrefix}.warningBudget.${actualKey}<=${maxKey}`);
  }
}

function finishSummaryValidation(problems, validationOptions = {}) {
  if (problems.length && validationOptions.throwOnError) {
    throw new Error(`Smoke matrix summary schema problems: ${problems.join(', ')}`);
  }
  return problems;
}

function makeSmokeMatrixSummarySelfTestArtifact() {
  return {
    schema: SMOKE_MATRIX_SUMMARY_SCHEMA,
    schemaVersion: 1,
    repositoryRoot: root,
    gitRevision: '0123456789abcdef0123456789abcdef01234567',
    source: {
      commit: '0123456789abcdef0123456789abcdef01234567',
      dirty: false,
      artifactHashes: {
        browserBundleSha256: '0'.repeat(64),
        smokeBinaryPath: ctoxBin,
        smokeBinarySha256: '1'.repeat(64),
      },
    },
    ctoxBin,
    resultPath: defaultResultPath,
    pagePath: '/index.html',
    requireEvidence: true,
    requestedModes: ['business-os-agent-scope-ui'],
    modes: [{
      mode: 'business-os-agent-scope-ui',
      attempts: [{
        attempt: 1,
        status: 0,
        signal: null,
        timedOut: false,
        timeoutMs: 300000,
        pagePath: '/index.html',
        url: 'http://127.0.0.1:61341/index.html',
        businessPort: 61341,
        signalingPort: 61342,
        durationMs: 12000,
        ok: true,
        error: null,
        processLifecycle: {
          schema: 'ctox.rxdb.smoke_process_lifecycle.v1',
          runId: 'matrix-self-test',
          mode: 'business-os-agent-scope-ui',
          parent: { pid: 1234, ppid: 1233, pgid: 1234 },
          startedAt: '2026-06-18T00:00:00.000Z',
          endedAt: '2026-06-18T00:00:12.000Z',
          startupPhase: 'cleanup',
          events: [{
            at: '2026-06-18T00:00:12.000Z',
            elapsedMs: 12000,
            type: 'smoke_cleanup_complete',
            phase: 'cleanup',
          }],
        },
        context: {
          authState: 'authenticated',
          actorRole: 'user',
          browserContext: 'clean',
          tenantScope: 'local-workspace',
          advancedStatus: 'business-os-advanced-status-v1',
        },
        evidenceKeys: ['advanced_status', 'business_os_agent_scope_auth_state'],
        evidence: {
          advanced_status: 'business-os-advanced-status-v1',
          business_os_agent_scope_auth_state: 'authenticated',
        },
        warningBudget: {
          browserWarnings: 0,
          maxBrowserWarnings: 0,
          browserErrors: 0,
          maxBrowserErrors: 0,
          websocketWarnings: 0,
          maxWebsocketWarnings: 5,
          requestFailures: 0,
          maxRequestFailures: 0,
          assetResponseErrors: 0,
          maxAssetResponseErrors: 0,
          startupReloads: 0,
          maxStartupReloads: 0,
          startupHookWaitMs: 50,
          maxStartupHookWaitMs: 60000,
          syncConfigWaitMs: 10,
          maxSyncConfigWaitMs: null,
          durationMs: 12000,
          maxDurationMs: null,
        },
        evidenceProblems: [],
      }],
      ok: true,
    }],
    configuration: {
      attempts: 1,
      modeTimeoutMs: 300000,
      businessPortBase: 61341,
      signalingPortBase: 61342,
      budgets: {},
    },
    startedAt: '2026-06-18T00:00:00.000Z',
    endedAt: '2026-06-18T00:00:12.000Z',
    ok: true,
  };
}

function failConfiguration(message) {
  summary.configurationError = message;
  console.error(`rxdb smoke matrix configuration error: ${message}`);
  writeSummary(false);
  process.exit(1);
}

function ensureCtoxSmokeBinary() {
  if (!fs.existsSync(ctoxBin)) {
    failConfiguration(`CTOX smoke binary does not exist at ${ctoxBin}; build it before running the matrix`);
  }
}

function parsePositiveIntegerConfig(name, value, options = {}) {
  const parsed = Number(value);
  const min = options.min ?? 1;
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    failConfiguration(`${name} must be an integer between ${min} and ${max}; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function parseNonNegativeIntegerConfig(name, value, options = {}) {
  const parsed = Number(value);
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (!Number.isInteger(parsed) || parsed < 0 || parsed > max) {
    failConfiguration(`${name} must be an integer between 0 and ${max}; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function parseOptionalNonNegativeIntegerConfig(name, value, options = {}) {
  if (value === undefined || value === null || value === '') return null;
  return parseNonNegativeIntegerConfig(name, value, options);
}

function parseSmokeEvidence(output) {
  const evidence = {};
  for (const line of String(output || '').split(/\r?\n/)) {
    const match = line.match(/^([a-zA-Z][a-zA-Z0-9_:-]*)=(.*)$/);
    if (!match) continue;
    const [, key, rawValue] = match;
    const value = rawValue.trim();
    if (!value) {
      evidence[key] = '';
      continue;
    }
    const numeric = Number(value);
    evidence[key] = Number.isFinite(numeric) && String(numeric) === value ? numeric : value;
  }
  return evidence;
}

function readTextFile(file) {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch {
    return '';
  }
}

function logTail(value, maxBytes = 16 * 1024) {
  const text = String(value || '');
  if (Buffer.byteLength(text, 'utf8') <= maxBytes) return text;
  let start = Math.max(0, text.length - maxBytes);
  while (start < text.length && Buffer.byteLength(text.slice(start), 'utf8') > maxBytes) start += 1;
  return `[truncated to final ${maxBytes} bytes]\n${text.slice(start)}`;
}

function removeTempPath(targetPath) {
  if (!targetPath) return;
  try {
    fs.rmSync(targetPath, {
      recursive: true,
      force: true,
      maxRetries: 3,
      retryDelay: 100,
    });
  } catch {
    // Keep cleanup best-effort; smoke failures are reported through status and evidence.
  }
}

function validateModeEvidence(mode, evidence) {
  const required = evidenceRequirementsForMode(mode);
  const problems = [];
  for (const key of required.keys) {
    if (!Object.prototype.hasOwnProperty.call(evidence, key)) {
      problems.push(key);
    }
  }
  for (const [key, expected] of Object.entries(required.values || {})) {
    if (evidence[key] !== expected) {
      problems.push(`${key}=${JSON.stringify(expected)}`);
    }
  }
  for (const [key, minimum] of Object.entries(required.minimums || {})) {
    if (!Number.isFinite(Number(evidence[key])) || Number(evidence[key]) < minimum) {
      problems.push(`${key}>=${minimum}`);
    }
  }
  for (const [key, maximum] of Object.entries(required.maximums || {})) {
    if (!Number.isFinite(Number(evidence[key])) || Number(evidence[key]) > maximum) {
      problems.push(`${key}<=${maximum}`);
    }
  }
  return problems;
}

function validateBrowserDiagnosticsBudget(evidence, budget) {
  const problems = [];
  if (budget.browserWarningBudget !== null) {
    const browserWarnings = Number(evidence.browser_warning_count || 0);
    if (Number.isFinite(browserWarnings) && browserWarnings > budget.browserWarningBudget) {
      problems.push(`browser_warning_count<=${budget.browserWarningBudget}`);
    }
  }
  if (budget.browserErrorBudget !== null) {
    const browserErrors = Number(evidence.browser_error_count || 0);
    if (Number.isFinite(browserErrors) && browserErrors > budget.browserErrorBudget) {
      problems.push(`browser_error_count<=${budget.browserErrorBudget}`);
    }
  }
  const websocketWarnings = Number(evidence.browser_websocket_warning_count || 0);
  if (Number.isFinite(websocketWarnings) && websocketWarnings > budget.websocketWarningBudget) {
    problems.push(`browser_websocket_warning_count<=${budget.websocketWarningBudget}`);
  }
  if (budget.requestFailureBudget !== null) {
    const requestFailures = Number(evidence.browser_request_failure_count || 0);
    if (Number.isFinite(requestFailures) && requestFailures > budget.requestFailureBudget) {
      problems.push(`browser_request_failure_count<=${budget.requestFailureBudget}`);
    }
  }
  const assetResponseErrors = Number(evidence.browser_asset_response_error_count || 0);
  if (Number.isFinite(assetResponseErrors) && assetResponseErrors > budget.assetResponseErrorBudget) {
    problems.push(`browser_asset_response_error_count<=${budget.assetResponseErrorBudget}`);
  }
  if (budget.syncConfigWaitBudgetMs !== null) {
    if (!Object.prototype.hasOwnProperty.call(evidence, 'ctox_sync_config_wait_ms')) {
      problems.push('ctox_sync_config_wait_ms');
    }
    const waitMs = Number(evidence.ctox_sync_config_wait_ms || 0);
    if (Number.isFinite(waitMs) && waitMs > budget.syncConfigWaitBudgetMs) {
      problems.push(`ctox_sync_config_wait_ms<=${budget.syncConfigWaitBudgetMs}`);
    }
  }
  return problems;
}

function validateDurationBudget(durationMs, maxDurationMs) {
  if (maxDurationMs === null) return [];
  return durationMs > maxDurationMs ? [`mode_duration_ms<=${maxDurationMs}`] : [];
}

function validateStartupBudget(evidence, budget) {
  const problems = [];
  if (!Object.prototype.hasOwnProperty.call(evidence, 'startup_smoke_hook_reload_count')) {
    problems.push('startup_smoke_hook_reload_count');
  }
  if (!Object.prototype.hasOwnProperty.call(evidence, 'startup_smoke_hook_wait_ms')) {
    problems.push('startup_smoke_hook_wait_ms');
  }
  const reloads = Number(evidence.startup_smoke_hook_reload_count || 0);
  if (Number.isFinite(reloads) && reloads > budget.startupReloadBudget) {
    problems.push(`startup_smoke_hook_reload_count<=${budget.startupReloadBudget}`);
  }
  const waitMs = Number(evidence.startup_smoke_hook_wait_ms || 0);
  if (Number.isFinite(waitMs) && waitMs > budget.startupHookWaitBudgetMs) {
    problems.push(`startup_smoke_hook_wait_ms<=${budget.startupHookWaitBudgetMs}`);
  }
  return problems;
}

function isFileChunkStatusMode(mode) {
  return mode === 'file-chunk-metadata-error-browser-status'
    || mode === 'file-chunk-tombstone-error-browser-status'
    || mode === 'file-chunk-stale-generation-error-browser-status';
}

function evidenceRequirementsForMode(mode) {
  const required = modeEvidenceRequirements[mode];
  if (!required) {
    throw new Error(`No smoke evidence requirements registered for mode=${mode}`);
  }
  return required;
}
