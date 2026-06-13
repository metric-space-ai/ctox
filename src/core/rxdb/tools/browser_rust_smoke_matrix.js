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
const { spawnSync } = require('child_process');
const os = require('os');

const toolPath = path.join(__dirname, 'browser_rust_smoke.js');
const root = path.resolve(__dirname, '../../../..');
const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const defaultModes = [
  'rust-to-browser',
  'browser-to-rust',
  'command-browser-to-rust',
  'tickets-browser-to-rust',
  'business-os-ui-regression',
  'browser-lifecycle-ui',
  'browser-handoff-ui',
  'migration-version-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-reload-browser-to-rust',
  'command-restart-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
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
      business_os_ui_secondary_opened_modules: 'matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets',
      business_os_ui_secondary_rendered_modules: 'matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets',
      business_os_ui_secondary_interacted_modules: 'matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets',
      business_os_ui_secondary_interaction_names: 'matching-list-matrix-tabs,conversations-channel-filter,outbound-compact-view-toggle,tickets-search-status-filter,shiftflow-center-tabs,buchhaltung-nav-switch,coding-agents-settings-modal,app-store-view-scope,browser-address-refresh,calendar-new-event-drawer,creator-expert-accordion,notes-nav-filter,reports-filter-controls,spreadsheets-search-filter',
      business_os_ui_interacted_modules: 'ctox,documents,knowledge,research',
      business_os_ui_interaction_names: 'ctox-zoom,documents-new-drawer,knowledge-tab-runbooks,knowledge-tab-data,knowledge-tab-skill,research-new-task-modal',
      business_os_ui_desktop_opened: 1,
      business_os_visual_workspace_visible: 1,
    },
  },
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
      file_integrity_error_code: 'ctox_file_chunk_missing',
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
      file_integrity_error_code: 'ctox_file_chunk_missing',
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
const knownModes = new Set(defaultModes);
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
const requestFailureBudgetInput = process.env.SMOKE_BROWSER_REQUEST_FAILURE_BUDGET;
const assetResponseErrorBudgetInput = process.env.SMOKE_BROWSER_ASSET_RESPONSE_ERROR_BUDGET || '0';
const networkFlapBrowserWarningBudgetInput = process.env.SMOKE_NETWORK_FLAP_BROWSER_WARNING_BUDGET;
const networkFlapRequestFailureBudgetInput = process.env.SMOKE_NETWORK_FLAP_REQUEST_FAILURE_BUDGET;
const modeDurationBudgetInput = process.env.SMOKE_MODE_DURATION_BUDGET_MS;
const syncConfigWaitBudgetInput = process.env.SMOKE_SYNC_CONFIG_WAIT_BUDGET_MS;
const resultPath = process.env.SMOKE_MATRIX_RESULT_PATH || '';
const requireEvidence = process.env.SMOKE_REQUIRE_EVIDENCE !== '0';
const summary = {
  pagePath,
  requireEvidence,
  modes: [],
  startedAt: new Date().toISOString(),
  endedAt: null,
  ok: false,
};
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
    const attemptRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-rxdb-smoke-'));
    env.CTOX_SMOKE_ROOT = attemptRoot;
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
    try { fs.unlinkSync(stdoutPath); } catch {}
    try { fs.unlinkSync(stderrPath); } catch {}
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
      businessPort: Number(env.BUSINESS_PORT),
      signalingPort: Number(env.SIGNALING_PORT),
      durationMs,
      ok: lastStatus === 0 && !lastSignal,
      error: result.error ? { code: result.error.code || '', message: result.error.message || '' } : null,
      evidence,
      warningBudget: {
        browserWarnings: Number(evidence.browser_warning_count || 0),
        maxBrowserWarnings: effectiveBrowserWarningBudget,
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

function writeSummary(ok) {
  summary.ok = ok;
  summary.endedAt = new Date().toISOString();
  if (!resultPath) return;
  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, `${JSON.stringify(summary, null, 2)}\n`);
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
