// Old-shell facade (Shell v4 R4, LOESCHPLAN): the generic permission model lives
// in business-permissions.js, which v4 apps load. This file only adds the
// per-collection read denials that app.js (and rxdb/tests) still read from here.
// In v4 these rights are app data: collections.<name>.rights in the owning app's
// manifest (modules/ctox, modules/nachweise V4-HANDLERS.md "Rechte"). Delete
// together with the old shell.
import { normalizeRole } from './roles.js?v=20260831-ctox-desktopapp-ports-v328';

export * from './business-permissions.js';

// Mirrors only the native role-wide denials in business_os::policy. Native
// actor, grant and scope checks remain authoritative for every collection.
const ADMIN_ONLY_COLLECTIONS = new Set([
  'business_users',
  'business_module_acl',
  'business_credentials',
  'business_module_source_files',
  'business_module_commits',
  'business_module_source_blob_chunks',
  'ctox_runtime_settings',
]);
const USER_PRIVATE_COCKPIT_COLLECTIONS = new Set([
  'ctox_harness_events',
  'ctox_runs',
  'ctox_crew_learnings',
]);

export function roleMayReadCollection(role, collection) {
  const normalizedRole = normalizeRole(role);
  if (normalizedRole === 'chef' || normalizedRole === 'admin') return true;
  if (ADMIN_ONLY_COLLECTIONS.has(collection)) return false;
  return normalizedRole !== 'user' || !USER_PRIVATE_COCKPIT_COLLECTIONS.has(collection);
}
