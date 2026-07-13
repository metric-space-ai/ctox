#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { execFile as execFileCallback } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const execFile = promisify(execFileCallback);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..', '..', '..');
const engineRoot = path.join(repoRoot, 'src', 'apps', 'business-os', 'office-engine');
const pinPath = path.join(engineRoot, 'upstream', 'euro-office-v9.3.1.json');
const patchPath = path.join(engineRoot, 'src', 'adapters', 'sdkjs-ctox-hooks.patch');
const sourceArg = process.argv.find((value) => value.startsWith('--source='));
const checkOnly = process.argv.includes('--check-only');
if (!sourceArg) throw new Error('Pass the existing pinned checkout root with --source=/absolute/path. This script never clones or fetches.');

const sourceRoot = path.resolve(sourceArg.slice('--source='.length));
const sdkjsRoot = path.join(sourceRoot, 'sdkjs');
const webAppsRoot = path.join(sourceRoot, 'web-apps');
const pin = JSON.parse(await readFile(pinPath, 'utf8'));
await assertHead(sdkjsRoot, pin.submodules.sdkjs, 'sdkjs');
await assertHead(webAppsRoot, pin.submodules['web-apps'], 'web-apps');

const patch = await readFile(patchPath);
const patchSha256 = createHash('sha256').update(patch).digest('hex');
const reverseCheck = await gitApplyCheck(sdkjsRoot, ['--reverse', '--check', patchPath]);
let status;
if (reverseCheck.ok) {
  status = 'already-applied';
} else {
  const forwardCheck = await gitApplyCheck(sdkjsRoot, ['--check', patchPath]);
  if (!forwardCheck.ok) {
    throw new Error(`CTOX sdkjs patch matches neither pristine nor patched pinned source.\nforward: ${forwardCheck.error}\nreverse: ${reverseCheck.error}`);
  }
  if (!checkOnly) await execFile('git', ['-C', sdkjsRoot, 'apply', patchPath]);
  status = checkOnly ? 'applicable' : 'applied';
}

console.log(JSON.stringify({
  schema_version: 'ctox-office-upstream-patch-result-v1',
  source_root: sourceRoot,
  sdkjs_sha: pin.submodules.sdkjs,
  web_apps_sha: pin.submodules['web-apps'],
  patch: path.relative(repoRoot, patchPath).replaceAll('\\', '/'),
  patch_sha256: patchSha256,
  check_only: checkOnly,
  status,
}, null, 2));

async function assertHead(root, expected, name) {
  const { stdout } = await execFile('git', ['-C', root, 'rev-parse', 'HEAD']);
  const actual = stdout.trim();
  if (actual !== expected) throw new Error(`${name} checkout SHA mismatch: expected ${expected}, got ${actual}`);
}

async function gitApplyCheck(root, args) {
  try {
    await execFile('git', ['-C', root, 'apply', ...args]);
    return { ok: true, error: '' };
  } catch (error) {
    return { ok: false, error: String(error.stderr || error.message || error).trim() };
  }
}
