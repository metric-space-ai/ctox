// Bounded, payload-free browser evidence. Process RSS and actual usability must
// be measured by the browser harness; JS heap samples are not renderer RSS.
export function createShellPerformanceTrace({
  clock = globalThis.performance,
  Observer = globalThis.PerformanceObserver,
  capacity = 64,
} = {}) {
  const phases = [];
  let droppedPhases = 0;
  let observedHeapPeakBytes = null;
  let observer = null;
  const longTasks = { count: 0, totalMs: 0, longestMs: 0 };
  const now = () => Number(clock?.now?.() || 0);
  const heap = () => {
    const used = Number(clock?.memory?.usedJSHeapSize);
    if (!Number.isFinite(used) || used <= 0) return null;
    observedHeapPeakBytes = Math.max(observedHeapPeakBytes || 0, used);
    return used;
  };
  try {
    if (Observer?.supportedEntryTypes?.includes('longtask')) {
      observer = new Observer(list => {
        for (const entry of list.getEntries()) {
          longTasks.count += 1;
          longTasks.totalMs += entry.duration;
          longTasks.longestMs = Math.max(longTasks.longestMs, entry.duration);
        }
      });
      observer.observe({ type: 'longtask', buffered: true });
    }
  } catch { observer?.disconnect(); observer = null; }

  return {
    async measure(name, action) {
      const startedMs = now();
      const heapBeforeBytes = heap();
      let completed = false;
      try {
        const value = await action();
        completed = true;
        return value;
      } finally {
        if (phases.length >= capacity) { phases.shift(); droppedPhases += 1; }
        phases.push({ name, startedMs, durationMs: Math.max(0, now() - startedMs), completed,
          heapBeforeBytes, heapAfterBytes: heap() });
      }
    },
    snapshot() {
      const navigation = clock?.getEntriesByType?.('navigation')?.[0];
      const assets = (clock?.getEntriesByType?.('resource') || [])
        .filter(entry => ['script', 'link', 'css'].includes(entry.initiatorType));
      return {
        phases: phases.map(entry => ({ ...entry })), droppedPhases,
        navigation: navigation ? {
          type: navigation.type, startMs: navigation.startTime,
          responseStartMs: navigation.responseStart, responseEndMs: navigation.responseEnd,
          domInteractiveMs: navigation.domInteractive,
        } : null,
        assets: { observedCount: assets.length,
          transferBytes: assets.reduce((sum, entry) => sum + Number(entry.transferSize || 0), 0),
          longestMs: assets.reduce((max, entry) => Math.max(max, entry.duration), 0) },
        longTasks: observer ? { ...longTasks } : null,
        jsHeap: { currentUsedBytes: heap(), observedPeakBytes: observedHeapPeakBytes },
        rendererRssBytes: null,
      };
    },
    stop() { observer?.disconnect(); },
  };
}
