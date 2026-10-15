import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { readModuleSchemaArtifact } from './module_schema_artifact.mjs';

test('declarative artifact is sufficient; executable schema is never a fallback', async (t) => {
  const dir = await mkdtemp(join(tmpdir(), 's9-schema-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const artifact = pathToFileURL(join(dir, 'collections.schema.json'));
  const value = { collections: { items: { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } } } } };
  globalThis.s9ExecutableSchemaRan = false;
  t.after(() => { delete globalThis.s9ExecutableSchemaRan; });
  await writeFile(join(dir, 'schema.js'), 'globalThis.s9ExecutableSchemaRan = true;');
  await writeFile(artifact, JSON.stringify(value));
  assert.deepEqual(await readModuleSchemaArtifact(artifact), value);
  assert.equal(globalThis.s9ExecutableSchemaRan, false);
  await rm(artifact);
  await assert.rejects(() => readModuleSchemaArtifact(artifact), { code: 'ENOENT' });
  assert.equal(globalThis.s9ExecutableSchemaRan, false, 'missing artifacts cannot execute app code');
});
