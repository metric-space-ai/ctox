// Build-time/legacy-upgrade input only. App schemas are data, never code run
// with the build process's filesystem or network authority.
import { readFile } from 'node:fs/promises';

export async function readModuleSchemaArtifact(url) {
  const document = JSON.parse(await readFile(url, 'utf8'));
  const collections = document?.collections;
  if (!collections || typeof collections !== 'object' || Array.isArray(collections)) {
    throw new Error(`Declarative schema artifact has no collections object: ${url}`);
  }
  for (const [name, schema] of Object.entries(collections)) {
    if (!name || !schema || typeof schema !== 'object' || Array.isArray(schema)) {
      throw new Error(`Invalid declarative collection ${name}: ${url}`);
    }
  }
  return document;
}
