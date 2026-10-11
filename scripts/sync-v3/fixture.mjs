import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { once } from 'node:events';
import { finished } from 'node:stream/promises';
import { join } from 'node:path';

export const fixturePath = new URL('./fixtures/thesen-scale.json', import.meta.url);
export async function loadFixture() {
  const specification = JSON.parse(await readFile(fixturePath, 'utf8'));
  validateFixture(specification);
  return specification;
}

export function validateFixture(specification) {
  if (specification?.version !== 1 || !Array.isArray(specification.collections)
      || specification.collections.length !== 4) throw new Error('Unsupported size fixture');
  const names = new Set();
  for (const item of specification.collections) {
    if (!/^[a-z][a-z0-9_]+$/.test(item.name) || names.has(item.name)
        || !Number.isSafeInteger(item.count) || item.count < 1 || item.count > 10000
        || !Number.isSafeInteger(item.documentBytes) || item.documentBytes < 512
        || item.documentBytes > 100000) throw new Error('Invalid or unbounded size fixture');
    names.add(item.name);
  }
}

// Size envelopes deliberately cannot be submitted as executable commands.
// The later isolated seed adapter must map them through actual native schemas.
export function fixtureDocument(collection, index) {
  if (!Number.isSafeInteger(index) || index < 0 || index >= collection.count) {
    throw new Error('Fixture row index outside collection');
  }
  const row = { id: `sync-v3-${collection.name}-${String(index).padStart(5, '0')}`,
    fixtureOnly: true, ordinal: index, padding: '' };
  const base = JSON.stringify(row);
  const paddingBytes = collection.documentBytes - Buffer.byteLength(base);
  if (paddingBytes < 0 || paddingBytes > 100000) throw new Error('Invalid fixture document size');
  // Repeated filler would compress away and hide the bulk-data cost. Use a
  // deterministic high-entropy ASCII payload; this is not customer content.
  const alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_';
  let state = createHash('sha256').update(row.id).digest().readUInt32LE(0) || 1;
  const padding = Buffer.alloc(paddingBytes);
  for (let offset = 0; offset < padding.length; offset += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    padding[offset] = alphabet.charCodeAt(state & 63);
  }
  row.padding = padding.toString('ascii');
  return row;
}

// Streams one document at a time, honours backpressure and refuses reused roots.
export async function writeFixture(outputDirectory) {
  const specification = await loadFixture();
  await mkdir(outputDirectory, { mode: 0o700 });
  const collections = [];
  for (const collection of specification.collections) {
    const output = createWriteStream(join(outputDirectory, `${collection.name}.ndjson`),
      { flags: 'wx', mode: 0o600 });
    const completion = finished(output);
    // Observe early I/O failures while the producer is still writing.
    completion.catch(() => {});
    const digest = createHash('sha256');
    let documentBytes = 0;
    try {
      for (let index = 0; index < collection.count; index += 1) {
        const document = JSON.stringify(fixtureDocument(collection, index));
        documentBytes += Buffer.byteLength(document);
        const line = `${document}\n`;
        digest.update(line);
        if (!output.write(line)) await once(output, 'drain');
      }
      output.end();
      await completion;
    } catch (error) {
      output.destroy();
      await completion.catch(() => {});
      throw error;
    }
    collections.push({ ...collection, file: `${collection.name}.ndjson`,
      totalDocumentBytes: documentBytes, ndjsonBytes: documentBytes + collection.count,
      sha256: digest.digest('hex') });
  }
  const manifest = { ...specification, collections, generated: true,
    productionSeed: false, installedAcceptance: false };
  await writeFile(join(outputDirectory, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`,
    { flag: 'wx', mode: 0o600 });
  return manifest;
}

export async function verifyFixture(outputDirectory) {
  const expected = await loadFixture();
  const manifest = JSON.parse(await readFile(join(outputDirectory, 'manifest.json'), 'utf8'));
  if (manifest.id !== expected.id || manifest.productionSeed !== false
      || manifest.collections?.length !== expected.collections.length) throw new Error('Fixture manifest mismatch');
  for (let index = 0; index < expected.collections.length; index += 1) {
    const collection = expected.collections[index];
    const recorded = manifest.collections[index];
    const file = `${collection.name}.ndjson`;
    if (recorded.name !== collection.name || recorded.file !== file
        || recorded.count !== collection.count || recorded.documentBytes !== collection.documentBytes) {
      throw new Error('Fixture collection manifest mismatch');
    }
    const digest = createHash('sha256');
    let bytes = 0;
    let rows = 0;
    let lineBytes = 0;
    for await (const chunk of createReadStream(join(outputDirectory, file))) {
      digest.update(chunk);
      bytes += chunk.length;
      for (const byte of chunk) {
        if (byte === 10) {
          if (lineBytes !== collection.documentBytes) throw new Error('Fixture document byte length mismatch');
          rows += 1;
          lineBytes = 0;
        } else {
          lineBytes += 1;
        }
      }
    }
    if (lineBytes !== 0 || rows !== collection.count
        || bytes !== collection.count * (collection.documentBytes + 1)
        || bytes !== recorded.ndjsonBytes || digest.digest('hex') !== recorded.sha256) {
      throw new Error('Fixture file digest/count mismatch');
    }
  }
  return { fixture: manifest.id, verified: true, installedAcceptance: false };
}
