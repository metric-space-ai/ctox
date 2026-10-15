import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { createHash } from 'node:crypto';
import { CtoxError } from './mango.mjs';

const fail = (code, message) => { throw new CtoxError({ code, message }); };
export async function assetText(manifest, assetPath, fixtures) {
  if (typeof assetPath !== 'string' || !assetPath || assetPath.includes('\\') ||
      assetPath.split('/').some(part => !part || part === '.' || part === '..' || part.includes(':') || part.includes('\0'))) {
    fail('invalid', 'App-relative asset path required');
  }
  const hash = manifest.files?.[assetPath];
  if (typeof hash !== 'string') fail('not_found', 'Asset is not in this app version');
  if (!/^[a-f0-9]{64}$/.test(hash)) fail('invalid', 'Invalid asset CAS hash');
  let bytes;
  if (fixtures.artifacts) {
    const artifact = fixtures.artifacts instanceof Map ? fixtures.artifacts.get(hash) : fixtures.artifacts[hash];
    if (artifact === undefined) fail('not_found', 'CAS asset unavailable');
    bytes = Buffer.from(artifact);
  } else {
    const root = await fs.realpath(fixtures.root ?? process.cwd());
    const file = await fs.realpath(path.join(root, assetPath));
    if (!file.startsWith(root + path.sep)) fail('forbidden', 'Asset escapes app package');
    bytes = await fs.readFile(file);
  }
  if (bytes.byteLength > 1024 * 1024) fail('quota', 'Text asset exceeds 1 MiB');
  if (createHash('sha256').update(bytes).digest('hex') !== hash) fail('conflict', 'Asset CAS integrity');
  try { return new TextDecoder('utf-8', { fatal:true }).decode(bytes); }
  catch { fail('invalid', 'Asset is not UTF-8'); }
}
