'use strict';

const requiredHealthChecks = [
  'authenticated', 'shellLoaded', 'activeModuleLoaded', 'workspaceNotLoading',
  'dataPlaneWebrtc', 'rxdbRuntimeAppLocal', 'moduleCatalogAvailable',
  'requiredCollectionsConnected', 'requiredCollectionsInitialSyncComplete',
  'requiredCollectionsStreamingReady', 'requiredCollectionsCheckpointEpochAdvertised',
  'noCheckpointProtocolErrors', 'noSchemaProtocolErrors', 'noReplicationIoErrors',
  'noFailedCollections', 'noStalledReconnect', 'frameTransportRealtimeHealthy',
  'noAutomaticRepairRunning',
];

// Pure fixture contract: visible shell and fully healthy readiness are separate
// milestones. The caller retains the 3s visibility and 70s readiness deadlines.
function startupReadiness(state, expectedModule, requiredStatusVersion) {
  const status = state?.advancedStatus;
  const validStatus = status?.version === requiredStatusVersion;
  const checks = validStatus ? status.checks || {} : {};
  const moduleReady = expectedModule
    ? state.activeModule === expectedModule || (state.windows || []).some(window =>
      window.ownerId === `desktop-app:${expectedModule}` && window.visible === true
      && window.moduleId === expectedModule && window.mountComplete === true
      && window.loadFailed === false && window.recovery === false && window.loading === false)
    : Boolean(state.activeModule);
  const bootValue = status?.shell?.bootTimings?.shellVisibleMs;
  const bootTimingMs = typeof bootValue === 'number' && Number.isFinite(bootValue) && bootValue >= 0
    ? bootValue : null;
  const shellVisible = !state.loading && state.shellVisible === true && state.moduleCount > 0
    && validStatus && checks.shellLoaded === true && checks.workspaceNotLoading === true;
  const connected = checks.dataPlaneWebrtc === true && checks.requiredCollectionsConnected === true;
  const healthy = validStatus && status.ok === true && requiredHealthChecks.every(key => checks[key] === true)
    && Object.values(checks).every(value => value === true);
  return { shellVisible, moduleReady, connected, healthy, bootTimingMs,
    ready: shellVisible && moduleReady && connected && healthy && bootTimingMs !== null
      && state.expectedTextFound === true };
}

function startupDiagnostics(state) {
  if (!state) return null;
  const sync = state.advancedStatus?.sync;
  const names = value => Array.isArray(value) ? value.filter(name =>
    typeof name === 'string' && /^[a-zA-Z0-9_-]{1,160}$/.test(name)).slice(0, 512) : null;
  return {
    activeModule: state.activeModule, loading: state.loading,
    shellVisible: state.shellVisible, windows: state.windows,
    moduleCount: state.moduleCount, expectedTextFound: state.expectedTextFound,
    timings: state.timings, dbBuild: state.dbBuild,
    fileConsumer: state.fileConsumer ? {
      phase: ['waiting', 'acquiring', 'active', 'failed', 'closed'].includes(state.fileConsumer.phase)
        ? state.fileConsumer.phase : 'unknown',
      collections: names(state.fileConsumer.collections),
    } : null,
    advancedStatus: state.advancedStatus ? {
      version: state.advancedStatus.version, ok: state.advancedStatus.ok,
      checks: state.advancedStatus.checks, failures: state.advancedStatus.failures,
      bootTimings: state.advancedStatus.shell?.bootTimings,
      // Attribution only: never copy connection/session/credential structures.
      sync: sync ? {
        requiredCollections: names(sync.requiredCollections),
        missingRequiredCollections: names(sync.missingRequiredCollections),
        requiredCollectionEvidence: Object.fromEntries((names(sync.requiredCollections) || []).map(name => {
          const row = sync.requiredCollectionEvidence?.[name];
          return [name, {
            hasCollection: typeof row?.hasCollection === 'boolean' ? row.hasCollection : null,
            hasData: typeof row?.hasData === 'boolean' ? row.hasData : null,
            readFailed: row ? typeof row.error === 'string' && row.error.length > 0 : null,
          }];
        })),
        collectionTotal: Number.isInteger(sync.collectionTotal) ? sync.collectionTotal : null,
        initialSync: {
          missingInitialReplication: names(sync.initialSync?.missingInitialReplication),
          missingStreamingReady: names(sync.initialSync?.missingStreamingReady),
          missingCheckpointEpoch: names(sync.initialSync?.missingCheckpointEpoch),
        },
      } : null,
    } : null,
  };
}

module.exports = { startupReadiness, startupDiagnostics };
