import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const root = new URL('../', import.meta.url);
const matrix = JSON.parse(await readFile(new URL('features.json', root), 'utf8'));
const pin = JSON.parse(await readFile(new URL(matrix.oracle_pin, root), 'utf8'));

assert.equal(matrix.schema_version, 'ctox-office-feature-matrix-v1');
assert.equal(pin.schema_version, 'ctox-euro-office-upstream-pin-v1');
assert.equal(pin.release, 'v9.3.1');
assert.equal(pin.production_runtime_allowed, false);
assert.equal(pin.automatic_fetch_allowed, false);
assert.match(pin.oracle_image.index_digest, /^sha256:[a-f0-9]{64}$/);

const allowedStatuses = new Set(matrix.status_order);
const features = Object.values(matrix.editors).flatMap((editor) => editor.features);
const byId = new Map(features.map((feature) => [feature.id, feature]));
assert.equal(byId.size, features.length, 'office feature ids must be unique');

for (const feature of features) {
  assert.ok(allowedStatuses.has(feature.status), `invalid status for ${feature.id}`);
  const statusIndex = matrix.status_order.indexOf(feature.status);
  if (statusIndex >= matrix.status_order.indexOf('oracle_captured')) {
    assert.ok(feature.evidence, `${feature.id} has no oracle evidence path`);
    assert.ok(feature.flow, `${feature.id} has no oracle flow path`);
    const evidence = JSON.parse(await readFile(new URL(feature.evidence, root), 'utf8'));
    const flow = JSON.parse(await readFile(new URL(feature.flow, root), 'utf8'));
    assert.equal(evidence.schema_version, 'ctox-office-oracle-evidence-v1');
    assert.equal(evidence.feature_id, feature.id);
    assert.equal(evidence.status, feature.status);
    assert.equal(flow.schema_version, 'ctox-office-oracle-flow-v1');
    assert.equal(flow.feature_id, feature.id);
    assert.ok(Array.isArray(evidence.source_anchors) && evidence.source_anchors.length > 0);
    assert.ok(Array.isArray(evidence.screenshots) && evidence.screenshots.length > 0);
  }
  for (const dependency of feature.depends_on || []) {
    assert.ok(byId.has(dependency), `${feature.id} has unknown dependency ${dependency}`);
    assert.notEqual(dependency, feature.id, `${feature.id} depends on itself`);
    if (statusIndex >= matrix.status_order.indexOf('frontend_ported')) {
      assert.ok(
        matrix.status_order.indexOf(byId.get(dependency).status) >= matrix.status_order.indexOf('differential_passed'),
        `${feature.id} started before dependency ${dependency} passed differential testing`,
      );
    }
  }
}

function visit(id, trail = []) {
  assert.ok(!trail.includes(id), `office feature dependency cycle: ${[...trail, id].join(' -> ')}`);
  for (const dependency of byId.get(id)?.depends_on || []) visit(dependency, [...trail, id]);
}
for (const id of byId.keys()) visit(id);

console.log(`CTOX Documents/Spreadsheets feature matrix OK (${features.length} features, oracle ${pin.release})`);
