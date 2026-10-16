// Positivfaelle: jede Regel genau hier mindestens einmal.
import { helper } from './helper.js?v=20261015';
import schema from './schema.js';

export async function mount(ctx) {
  await ctx.sync.startCollection('demo');
  await ctx.sync.stopCollection('demo');
  await ctx.sync.restartCollection('demo');
  await ctx.sync.leaseCollection('demo');
  const lease = ctx.sync.leaseModule('demo');
  const activeModuleSyncLease = lease;
  const off = ctx.sync.subscribeCollectionReadiness('demo', () => {});
  ctx.sync.requireRevision('demo', 3);
  if (!ctx.sync.localReplicaComplete) return;
  await ctx.sync.requestNative({ op: 'x' });
  await ctx.readNativeCollectionDocument('demo', 'a');
  await ctx.readCollectionNativeDocument('demo', 'a');
  if (!ctx.dataPlaneReady) await ctx.waitForDataPlaneReady();
  const advancedStatus = {};
  const recoveryWarning = null;
  window.syncDiagnostics = {};
  startMaintenanceMonitor();
  clearTimeout(ctoxHealthTimer);
  await ctx.commandBus.dispatch({ type: 'x' }, { until: 'accepted' });
  await ctx.commandBus.dispatch({ type: 'y' }, { until: "terminal" });
  location.reload();
  const timer = setInterval(() => {}, 1000);
  await fetch('/api/business/demo');
  await fetch('https://example.invalid/x');
  await fetch(`//cdn.invalid/x`);
  const list = await fetch('../registry.json');
  const fallback = OFFLINE_FALLBACK_CATALOG;
  wireShellV2ModuleTitle(root);
  const guard = createDynamicAppDataGuard(ctx);
  if (!hasReviewedCollectionDataGrant(ctx, 'demo')) return;
  root.innerHTML = '<div class="ctox-syncing" role="status">Daten werden synchronisiert.</div>';
  root.title = `Gespeicherter Stand vom ${new Date()} wird mit CTOX abgeglichen`;
  setLabel('Stand unbestätigt');
  setLabel(lang === 'en' ? 'Syncing data…' : 'Inhalte werden geladen');
  setLabel('Loading workspace');
  setLabel('Offline');
  throw new Error('Die Daten wurden nicht rechtzeitig geladen.');
}
