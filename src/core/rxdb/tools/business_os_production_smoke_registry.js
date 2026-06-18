#!/usr/bin/env node
'use strict';

const businessOsProductionSmokeModes = Object.freeze([
  'business-os-app-release-ui',
  'business-os-app-audience-ui',
  'business-os-agent-scope-ui',
  'business-os-auth-scope-ui',
  'business-os-fresh-profile-ui',
  'business-os-restore-resync-ui',
]);

const businessOsProductionSmokeModeSet = new Set(businessOsProductionSmokeModes);

const businessOsProductionSmokeEvidenceRequirements = Object.freeze({
  'business-os-app-release-ui': freezeRequirement({
    keys: [
      'business_os_app_release_target_module',
      'business_os_app_release_actor_role',
      'business_os_app_release_auth_state',
      'business_os_app_release_browser_context',
      'business_os_app_release_tenant_scope',
      'business_os_app_release_private_before_release',
      'business_os_app_release_publish_succeeded',
      'business_os_app_release_team_visible_after_release',
      'business_os_app_release_version_badge_visible',
      'business_os_app_release_data_review_visible',
      'business_os_app_release_rollback_succeeded',
      'business_os_app_release_release_audit_visible',
      'business_os_app_release_rollback_audit_visible',
      'business_os_app_release_activity_audit_redacted',
      'business_os_app_release_reload_verified',
      'business_os_app_release_storage_boundary_checked',
      'advanced_status',
    ],
    values: {
      business_os_app_release_auth_state: 'authenticated',
      business_os_app_release_browser_context: 'clean',
      business_os_app_release_private_before_release: 1,
      business_os_app_release_publish_succeeded: 1,
      business_os_app_release_team_visible_after_release: 1,
      business_os_app_release_version_badge_visible: 1,
      business_os_app_release_data_review_visible: 1,
      business_os_app_release_rollback_succeeded: 1,
      business_os_app_release_release_audit_visible: 1,
      business_os_app_release_rollback_audit_visible: 1,
      business_os_app_release_activity_audit_redacted: 1,
      business_os_app_release_reload_verified: 1,
      business_os_app_release_storage_boundary_checked: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
  }),
  'business-os-app-audience-ui': freezeRequirement({
    keys: [
      'business_os_app_audience_target_module',
      'business_os_app_audience_actor_role',
      'business_os_app_audience_auth_state',
      'business_os_app_audience_browser_context',
      'business_os_app_audience_tenant_scope',
      'business_os_app_audience_private_hidden_for_team',
      'business_os_app_audience_preview_visible_for_target',
      'business_os_app_audience_preview_hidden_for_outside',
      'business_os_app_audience_restricted_hidden_for_outside',
      'business_os_app_audience_deep_link_locked_outside',
      'business_os_app_audience_reload_verified',
      'business_os_app_audience_fresh_profile_verified',
      'business_os_app_audience_storage_boundary_checked',
      'advanced_status',
    ],
    values: {
      business_os_app_audience_auth_state: 'authenticated',
      business_os_app_audience_browser_context: 'clean',
      business_os_app_audience_private_hidden_for_team: 1,
      business_os_app_audience_preview_visible_for_target: 1,
      business_os_app_audience_preview_hidden_for_outside: 1,
      business_os_app_audience_restricted_hidden_for_outside: 1,
      business_os_app_audience_deep_link_locked_outside: 1,
      business_os_app_audience_reload_verified: 1,
      business_os_app_audience_fresh_profile_verified: 1,
      business_os_app_audience_storage_boundary_checked: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
  }),
  'business-os-agent-scope-ui': freezeRequirement({
    keys: [
      'business_os_agent_scope_target_module',
      'business_os_agent_scope_agent_id',
      'business_os_agent_scope_actor_role',
      'business_os_agent_scope_auth_state',
      'business_os_agent_scope_browser_context',
      'business_os_agent_scope_tenant_scope',
      'business_os_agent_scope_panel_visible',
      'business_os_agent_scope_client_context_matches_ui',
      'business_os_agent_scope_app_store_panel_visible',
      'business_os_agent_scope_app_store_context_matches_ui',
      'business_os_agent_scope_business_chat_scope_matches_context',
      'business_os_agent_scope_settings_grant_boundary_visible',
      'business_os_agent_scope_app_hidden_denied',
      'business_os_agent_scope_data_denied_before_grant',
      'business_os_agent_scope_read_allowed_after_grant',
      'business_os_agent_scope_write_denied_without_grant',
      'business_os_agent_scope_audit_visible',
      'business_os_agent_scope_denied_reason_visible',
      'advanced_status',
    ],
    values: {
      business_os_agent_scope_auth_state: 'authenticated',
      business_os_agent_scope_browser_context: 'clean',
      business_os_agent_scope_panel_visible: 1,
      business_os_agent_scope_client_context_matches_ui: 1,
      business_os_agent_scope_app_store_panel_visible: 1,
      business_os_agent_scope_app_store_context_matches_ui: 1,
      business_os_agent_scope_business_chat_scope_matches_context: 1,
      business_os_agent_scope_settings_grant_boundary_visible: 1,
      business_os_agent_scope_app_hidden_denied: 1,
      business_os_agent_scope_data_denied_before_grant: 1,
      business_os_agent_scope_read_allowed_after_grant: 1,
      business_os_agent_scope_write_denied_without_grant: 1,
      business_os_agent_scope_audit_visible: 1,
      business_os_agent_scope_denied_reason_visible: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
  }),
  'business-os-auth-scope-ui': freezeRequirement({
    keys: [
      'business_os_auth_login_verified',
      'business_os_auth_authenticated_reload_verified',
      'business_os_auth_logout_verified',
      'business_os_auth_logged_out_reload_blocked',
      'business_os_auth_protected_access_blocked',
      'business_os_auth_tenant_scope_verified',
      'business_os_auth_browser_context_clean',
      'business_os_auth_cross_scope_storage_denied',
      'business_os_auth_tenant_scope_claim',
      'business_os_auth_storage_copy_did_not_widen_scope',
      'business_os_auth_final_state',
      'business_os_auth_auth_state',
      'business_os_auth_actor_role',
      'business_os_auth_browser_context',
      'business_os_auth_tenant_scope',
      'advanced_status',
    ],
    values: {
      business_os_auth_login_verified: 1,
      business_os_auth_authenticated_reload_verified: 1,
      business_os_auth_logout_verified: 1,
      business_os_auth_logged_out_reload_blocked: 1,
      business_os_auth_protected_access_blocked: 1,
      business_os_auth_tenant_scope_verified: 1,
      business_os_auth_browser_context_clean: 1,
      business_os_auth_cross_scope_storage_denied: 1,
      business_os_auth_tenant_scope_claim: 'local-workspace-only',
      business_os_auth_storage_copy_did_not_widen_scope: 1,
      business_os_auth_final_state: 'logged_out',
      business_os_auth_auth_state: 'logged_out',
      business_os_auth_browser_context: 'clean',
      advanced_status: 'business-os-advanced-status-v1',
    },
  }),
  'business-os-fresh-profile-ui': freezeRequirement({
    keys: [
      'business_os_fresh_profile_clean_indexeddb',
      'business_os_fresh_profile_clean_local_storage',
      'business_os_fresh_profile_clean_session_storage',
      'business_os_fresh_profile_authoritative_projection_loaded',
      'business_os_fresh_profile_lifecycle_labels_visible',
      'business_os_fresh_profile_version_badges_visible',
      'business_os_fresh_profile_disabled_reasons_visible',
      'business_os_fresh_profile_desktop_viewport_verified',
      'business_os_fresh_profile_narrow_viewport_verified',
      'business_os_fresh_profile_no_storage_widening',
      'business_os_fresh_profile_auth_state',
      'business_os_fresh_profile_actor_role',
      'business_os_fresh_profile_browser_context',
      'business_os_fresh_profile_tenant_scope',
      'business_os_fresh_profile_scale_fixture_modules',
      'business_os_fresh_profile_scale_catalog_modules',
      'business_os_fresh_profile_scale_explicit_grants',
      'business_os_fresh_profile_scale_release_versions',
      'business_os_fresh_profile_scale_native_permission_grants',
      'business_os_fresh_profile_scale_native_module_versions',
      'business_os_fresh_profile_scale_native_audit_events',
      'business_os_fresh_profile_scale_app_store_cards',
      'business_os_fresh_profile_scale_render_ms',
      'business_os_fresh_profile_scale_start_menu_ms',
      'business_os_fresh_profile_scale_app_store_ms',
      'business_os_fresh_profile_scale_budget_passed',
      'advanced_status',
    ],
    values: {
      business_os_fresh_profile_clean_indexeddb: 1,
      business_os_fresh_profile_clean_local_storage: 1,
      business_os_fresh_profile_clean_session_storage: 1,
      business_os_fresh_profile_authoritative_projection_loaded: 1,
      business_os_fresh_profile_lifecycle_labels_visible: 1,
      business_os_fresh_profile_version_badges_visible: 1,
      business_os_fresh_profile_disabled_reasons_visible: 1,
      business_os_fresh_profile_desktop_viewport_verified: 1,
      business_os_fresh_profile_narrow_viewport_verified: 1,
      business_os_fresh_profile_no_storage_widening: 1,
      business_os_fresh_profile_auth_state: 'authenticated',
      business_os_fresh_profile_browser_context: 'clean',
      business_os_fresh_profile_scale_budget_passed: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
    minimums: {
      business_os_fresh_profile_scale_fixture_modules: 32,
      business_os_fresh_profile_scale_catalog_modules: 50,
      business_os_fresh_profile_scale_explicit_grants: 64,
      business_os_fresh_profile_scale_release_versions: 96,
      business_os_fresh_profile_scale_native_permission_grants: 64,
      business_os_fresh_profile_scale_native_module_versions: 96,
      business_os_fresh_profile_scale_native_audit_events: 128,
      business_os_fresh_profile_scale_app_store_cards: 20,
    },
    maximums: {
      business_os_fresh_profile_scale_render_ms: 5000,
      business_os_fresh_profile_scale_start_menu_ms: 5000,
      business_os_fresh_profile_scale_app_store_ms: 15000,
    },
  }),
  'business-os-restore-resync-ui': freezeRequirement({
    keys: [
      'business_os_restore_resync_auth_state',
      'business_os_restore_resync_actor_role',
      'business_os_restore_resync_browser_context',
      'business_os_restore_resync_tenant_scope',
      'business_os_restore_resync_webrtc_only',
      'business_os_restore_resync_peer_stopped',
      'business_os_restore_resync_local_only_before_restart',
      'business_os_restore_resync_peer_restarted',
      'business_os_restore_resync_checkpoint_epoch_count',
      'business_os_restore_resync_native_converged_after_restart',
      'business_os_restore_resync_replicated_id',
      'advanced_status',
    ],
    values: {
      business_os_restore_resync_auth_state: 'authenticated',
      business_os_restore_resync_browser_context: 'clean',
      business_os_restore_resync_tenant_scope: 'local-workspace',
      business_os_restore_resync_webrtc_only: 1,
      business_os_restore_resync_peer_stopped: 1,
      business_os_restore_resync_local_only_before_restart: 1,
      business_os_restore_resync_peer_restarted: 1,
      business_os_restore_resync_native_converged_after_restart: 1,
      advanced_status: 'business-os-advanced-status-v1',
    },
    minimums: {
      business_os_restore_resync_checkpoint_epoch_count: 1,
    },
  }),
});

function freezeRequirement(requirement) {
  return Object.freeze({
    keys: Object.freeze([...(requirement.keys || [])]),
    values: Object.freeze({ ...(requirement.values || {}) }),
    minimums: Object.freeze({ ...(requirement.minimums || {}) }),
    maximums: Object.freeze({ ...(requirement.maximums || {}) }),
  });
}

function asSet(values) {
  if (values instanceof Set) return values;
  return new Set(values || []);
}

function assertBusinessOsProductionSmokeRegistry({ runnerModes, matrixModes, modeEvidenceRequirements }) {
  const runnerModeSet = asSet(runnerModes);
  const matrixModeSet = asSet(matrixModes);
  const problems = [];
  for (const mode of businessOsProductionSmokeModes) {
    if (!runnerModeSet.has(mode)) {
      problems.push(`runner:${mode}`);
    }
    if (!matrixModeSet.has(mode)) {
      problems.push(`matrix:${mode}`);
    }
    const requirement = modeEvidenceRequirements?.[mode];
    if (!requirement || !Array.isArray(requirement.keys) || requirement.keys.length === 0) {
      problems.push(`evidence:${mode}`);
      continue;
    }
    const keySet = new Set(requirement.keys);
    for (const key of Object.keys(requirement.values || {})) {
      if (!keySet.has(key)) problems.push(`evidence-value-key:${mode}:${key}`);
    }
    for (const key of Object.keys(requirement.minimums || {})) {
      if (!keySet.has(key)) problems.push(`evidence-minimum-key:${mode}:${key}`);
    }
    for (const key of Object.keys(requirement.maximums || {})) {
      if (!keySet.has(key)) problems.push(`evidence-maximum-key:${mode}:${key}`);
    }
  }
  if (problems.length) {
    throw new Error(`Business OS production smoke registry is incomplete: ${problems.join(', ')}`);
  }
}

module.exports = {
  assertBusinessOsProductionSmokeRegistry,
  businessOsProductionSmokeEvidenceRequirements,
  businessOsProductionSmokeModes,
  businessOsProductionSmokeModeSet,
};
