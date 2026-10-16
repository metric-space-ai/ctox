// Legacy-data migration fixtures consume declarative artifacts, never execute
// an app's schema module. The native contract remains an independent parity pin.
import { readModuleSchemaArtifact } from '../../../../core/rxdb/tools/module_schema_artifact.mjs';

export async function packagedSchema(appId) {
  if (!/^[a-z0-9][a-z0-9-]*$/.test(appId)) throw new Error('Invalid app fixture identity');
  const value = await readModuleSchemaArtifact(new URL(`../../modules/${appId}/collections.schema.json`, import.meta.url));
  if (!value.collections || !value.migration_strategies) throw new Error(`Incomplete schema artifact: ${appId}`);
  return value;
}
