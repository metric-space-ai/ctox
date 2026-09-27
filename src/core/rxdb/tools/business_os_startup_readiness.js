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
      window.ownerId === `desktop-app:${expectedModule}` && window.visible === true)
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
  return {
    activeModule: state.activeModule, loading: state.loading,
    shellVisible: state.shellVisible, windows: state.windows,
    moduleCount: state.moduleCount, expectedTextFound: state.expectedTextFound,
    timings: state.timings, dbBuild: state.dbBuild,
    advancedStatus: state.advancedStatus ? {
      version: state.advancedStatus.version, ok: state.advancedStatus.ok,
      checks: state.advancedStatus.checks, failures: state.advancedStatus.failures,
      bootTimings: state.advancedStatus.shell?.bootTimings,
    } : null,
  };
}

module.exports = { startupReadiness, startupDiagnostics };
