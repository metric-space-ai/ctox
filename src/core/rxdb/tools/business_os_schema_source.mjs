// Packaged browser-test creators consume the same generated v4 contract as native.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { normalizeCollectionsSchema } from '../../../apps/business-os/v4/tools/manifest/gen-manifest.mjs';
export async function readPackagedModuleDefinitions(root, moduleId) {
  const read = async (...parts) => JSON.parse(await readFile(join(root, ...parts), 'utf8'));
  const own = normalizeCollectionsSchema(await read('src/apps/business-os/v4/schemas', `${moduleId}.collections.schema.json`));
  const legacy = normalizeCollectionsSchema(await read('src/apps/business-os/modules', moduleId, 'collections.schema.json'));
  const canonical = await read('src/core/business_os/business_os_schema_contract.json');
  const result = {};
  // Preserve declared foreign read exports (e.g. ctox -> platform commands),
  // without taking their obsolete schema copies as another source.
  for (const name of new Set([...Object.keys(own.collections), ...Object.keys(legacy.collections)])) {
    if (!canonical[name]) throw new Error(`Missing canonical v4 schema: ${moduleId}/${name}`);
    result[name] = { ...(legacy.wrappers[name] || {}), schema: canonical[name] };
  }
  return result;
}
