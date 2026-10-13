#!/usr/bin/env node
import { writeFixture, verifyFixture, loadFixture, fixtureDocument } from './sync-v3/fixture.mjs';

const [operation, output, ...extra] = process.argv.slice(2);
try {
  if (extra.length || !['--fixture', '--verify-fixture', '--validate-fixture'].includes(operation)
      || (operation !== '--validate-fixture' ? !output : output !== undefined)) {
    throw new Error('Usage: node scripts/measure-sync-v3.mjs --fixture NEW_DIRECTORY | --verify-fixture DIRECTORY | --validate-fixture');
  }
  if (operation === '--fixture') {
    const manifest = await writeFixture(output);
    console.log(JSON.stringify(manifest));
  } else if (operation === '--verify-fixture') {
    console.log(JSON.stringify(await verifyFixture(output)));
  } else {
    const specification = await loadFixture();
    const collections = specification.collections.map((collection) => {
      for (const index of [0, collection.count - 1]) {
        if (Buffer.byteLength(JSON.stringify(fixtureDocument(collection, index))) !== collection.documentBytes) {
          throw new Error(`Fixture byte count mismatch: ${collection.name}`);
        }
      }
      return { name: collection.name, count: collection.count,
        documentBytes: collection.documentBytes, totalDocumentBytes: collection.count * collection.documentBytes };
    });
    console.log(JSON.stringify({ fixture: specification.id, valid: true, collections,
      totalDocumentBytes: collections.reduce((total, collection) => total + collection.totalDocumentBytes, 0),
      installedAcceptance: false }));
  }
} catch (error) {
  console.error(`measure-sync-v3: ${error.message}`);
  process.exitCode = 1;
}
