// SYNC-13: the browser consumes the per-collection `syncProfile` declaration.
//
// SYNC-32 added `syncProfile` ("eager" | "demand-only" | "demand-chunks") as a
// sibling of `schema`, consumed natively. The browser used to hardcode its
// demand-only / demand-chunk classification. Now rx-database.mjs's
// `addCollections` captures each collection's declared profile into a
// globalThis-mirrored registry (populated at registration, before sync starts),
// and shared/sync.js's demand classifiers consult it, FALLING BACK to their
// built-in static lists.
//
// Contract pinned here:
//   1. an UNDECLARED collection keeps its built-in classification (nothing
//      regresses for static collections);
//   2. a runtime collection declaring `syncProfile: 'demand-only'` is treated
//      as pull-demand-only but stays module-startable (like user_threads);
//   3. `demand-chunks` is BOTH pull-demand-only AND module-demand-only
//      (skipped at module sync startup, leased on demand);
//   4. `eager` / unknown declarations are stored as "no override" — identical
//      to an undeclared collection;
//   5. built-in demand collections are classified by the built-in list
//      regardless of the registry.

import assert from 'node:assert/strict';
import {
  registerCollectionSyncProfile,
  getCollectionSyncProfile,
  clearCollectionSyncProfiles,
} from '../dist/ctox-rxdb-js.mjs';
import { __ctoxSyncTestHooks } from '../../shared/sync.js';

const { isDemandOnlyPullCollection, isModuleDemandOnlyCollection } = __ctoxSyncTestHooks;

clearCollectionSyncProfiles();

// --- 1. undeclared runtime collection keeps built-in (eager) classification --
assert.equal(isDemandOnlyPullCollection('acme_records'), false, 'undeclared runtime collection pulls eagerly');
assert.equal(isModuleDemandOnlyCollection('acme_records'), false, 'undeclared runtime collection is module-startable');
assert.equal(getCollectionSyncProfile('acme_records'), null, 'undeclared collection has no registry entry');

// --- 2. demand-only: pull disabled, still module-startable --------------------
registerCollectionSyncProfile('acme_records', 'demand-only');
assert.equal(getCollectionSyncProfile('acme_records'), 'demand-only', 'demand-only declaration is registered');
assert.equal(isDemandOnlyPullCollection('acme_records'), true, 'a demand-only runtime collection is pull-demand-only');
assert.equal(isModuleDemandOnlyCollection('acme_records'), false, 'a demand-only collection stays module-startable');

// --- 3. demand-chunks: pull-demand-only AND module-demand-only ---------------
registerCollectionSyncProfile('acme_blob_chunks', 'demand-chunks');
assert.equal(getCollectionSyncProfile('acme_blob_chunks'), 'demand-chunks', 'demand-chunks declaration is registered');
assert.equal(isDemandOnlyPullCollection('acme_blob_chunks'), true, 'a demand-chunks runtime collection is pull-demand-only');
assert.equal(isModuleDemandOnlyCollection('acme_blob_chunks'), true, 'a demand-chunks runtime collection is module-demand-only');

// --- 4. eager / unknown declarations are stored as "no override" -------------
registerCollectionSyncProfile('acme_eager', 'eager');
assert.equal(getCollectionSyncProfile('acme_eager'), null, 'eager is stored as no-override');
assert.equal(isDemandOnlyPullCollection('acme_eager'), false, 'an eager collection pulls eagerly');
registerCollectionSyncProfile('acme_bogus', 'nonsense');
assert.equal(getCollectionSyncProfile('acme_bogus'), null, 'an unknown profile is stored as no-override');
assert.equal(isDemandOnlyPullCollection('acme_bogus'), false, 'an unknown-profile collection pulls eagerly');
// A later re-registration with a demand profile is authoritative.
registerCollectionSyncProfile('acme_eager', 'demand-only');
assert.equal(isDemandOnlyPullCollection('acme_eager'), true, 're-registration with a demand profile is authoritative');
registerCollectionSyncProfile('acme_eager', 'eager');
assert.equal(isDemandOnlyPullCollection('acme_eager'), false, 're-registration back to eager clears the override');

// --- 5. built-in collections are unaffected by the registry ------------------
assert.equal(isDemandOnlyPullCollection('desktop_file_chunks'), true, 'built-in chunk collection stays pull-demand-only');
assert.equal(isModuleDemandOnlyCollection('desktop_file_chunks'), true, 'built-in chunk collection stays module-demand-only');
assert.equal(isDemandOnlyPullCollection('user_threads'), true, 'built-in demand-only thread collection stays demand-only');
assert.equal(isModuleDemandOnlyCollection('user_threads'), false, 'built-in thread collection stays module-startable');
assert.equal(isDemandOnlyPullCollection('desktop_files'), false, 'built-in eager collection stays eager');
// The built-in list is authoritative: a conflicting registry entry can never
// demote a built-in demand collection back to eager pull (nothing regresses).
registerCollectionSyncProfile('desktop_file_chunks', 'eager');
assert.equal(isDemandOnlyPullCollection('desktop_file_chunks'), true, 'a stray eager declaration cannot demote a built-in demand collection');
assert.equal(isModuleDemandOnlyCollection('desktop_file_chunks'), true, 'a stray eager declaration cannot demote a built-in module-demand collection');

clearCollectionSyncProfiles();
assert.equal(getCollectionSyncProfile('acme_records'), null, 'clear resets the registry');

console.log('ctox-rxdb sync-profile-registry smoke OK');
process.exit(0);
