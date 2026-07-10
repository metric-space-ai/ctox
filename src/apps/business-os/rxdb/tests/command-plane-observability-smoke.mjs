import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createCommandBus, resetBusinessOsCapabilityTokenCacheForTests } from '../../shared/command-bus.js';
import { createSyncRuntime } from '../../shared/sync.js';

resetBusinessOsCapabilityTokenCacheForTests();
globalThis.CTOX_BUSINESS_OS_SESSION = {
  capability_token: 'observability-capability-token',
  capability_expires_at_ms: Date.now() + 60 * 60 * 1000,
};
const documents = new Map();
const collection = {
  async insert(document) {
    await new Promise((resolve) => setTimeout(resolve, 1));
    documents.set(document.id, { ...document });
  },
  findOne(id) {
    return {
      async exec() {
        const document = documents.get(id);
        return document ? { toJSON: () => ({ ...document }) } : null;
      },
    };
  },
};
const db = { mode: 'rxdb', raw: { business_commands: collection, ctox_queue_tasks: collection } };
const syncRuntime = createSyncRuntime({
  db,
  config: {
    transport: 'webrtc',
    sync_room: 'command-plane-observability',
    signaling_urls: ['ws://127.0.0.1:19000'],
  },
  onDiagnostic() {},
});
const metricsOnlySync = {
  recordCommandMetric(metric) {
    syncRuntime.recordCommandMetric(metric);
  },
};
const bus = createCommandBus({ db, sync: metricsOnlySync });

for (let index = 0; index < 20; index += 1) {
  await bus.submit({
    id: `cmd-observability-${index}`,
    module: 'ctox',
    command_type: 'business_os.test',
    payload: { index },
  });
}

const commandPlane = syncRuntime.diagnostics.commandPlane;
assert.equal(commandPlane.counters.local_submit, 20);
assert.equal(commandPlane.counters.submit_receipt, 20);
assert.equal(commandPlane.latency.local_submit.samples, 20);
assert.ok(commandPlane.latency.local_submit.p95Ms >= 0);
assert.equal(commandPlane.commandTriggeredRestarts, 0);

const baseline = {
  schema: 'ctox.command_plane.ci_baseline.v1',
  captured_at: new Date().toISOString(),
  environment: 'deterministic-indexeddb-adapter-smoke',
  command_plane: commandPlane,
  provisional_targets: {
    local_submit_p95_ms: 100,
    command_triggered_restarts: 0,
  },
};
const outputPath = process.env.COMMAND_PLANE_BASELINE_PATH;
if (outputPath) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(baseline, null, 2)}\n`);
}
console.log('command-plane observability smoke OK', baseline);
