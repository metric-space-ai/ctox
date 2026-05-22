import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const scriptDir = import.meta.dirname;
const srcRoot = path.resolve(scriptDir, '..', '..');
const repoRoot = path.resolve(srcRoot, '..');
const rxdbRoot = path.join(srcRoot, 'core', 'rxdb');
const bundlePath = path.join(srcRoot, 'apps', 'business-os', 'vendor', 'rxdb-bundle.mjs');
const provenancePath = path.join(srcRoot, 'apps', 'business-os', 'vendor', 'rxdb-bundle.provenance.json');
const sourcePackagePath = path.join(rxdbRoot, 'js-fork', 'source', 'package.json');
const contractPath = path.join(rxdbRoot, 'js-fork', 'bundle-contract.json');
const guardPath = path.join(rxdbRoot, 'tools', 'assert_js_fork_bundle_contract.js');
const buildDir = path.join(repoRoot, 'runtime', 'build', 'ctox-rxdb-js');
const artifactDir = readOption('--artifact-dir');
const builtBundlePath = path.join(artifactDir ? path.resolve(repoRoot, artifactDir) : buildDir, 'rxdb-bundle.mjs');
const writeProvenance = process.argv.includes('--write-provenance');
const writeBundle = process.argv.includes('--write-bundle');

const sourcePackage = JSON.parse(await fs.readFile(sourcePackagePath, 'utf8'));
const contract = JSON.parse(await fs.readFile(contractPath, 'utf8'));
const sourceLockfilePath = path.join(rxdbRoot, contract.source_lockfile || 'js-fork/source/package-lock.json');
const entryPath = path.join(rxdbRoot, contract.build_entry);
const outfile = writeBundle ? bundlePath : builtBundlePath;
const provenanceOutfile = writeBundle
  ? provenancePath
  : path.join(path.dirname(outfile), 'rxdb-bundle.provenance.json');
await fs.mkdir(path.dirname(outfile), { recursive: true });
await buildBundle(entryPath, outfile);
const bundle = await fs.readFile(outfile);
const sourceLockfile = await fs.readFile(sourceLockfilePath);
const provenance = {
  name: 'ctox-rxdb-js-browser-bundle',
  source_package: path.relative(repoRoot, sourcePackagePath),
  source_lockfile: path.relative(repoRoot, sourceLockfilePath),
  source_lock_sha256: createHash('sha256').update(sourceLockfile).digest('hex'),
  source_name: sourcePackage.name,
  source_version: sourcePackage.version,
  package_manager: contract.package_manager || 'npm-ci',
  publish_policy: contract.publish_policy || sourcePackage.ctoxHardFork?.publishPolicy?.npm || 'private-package-only',
  version_discipline: contract.version_discipline || 'upstream-version-pinned-with-ctox-provenance',
  upstream_repository: sourcePackage.ctoxHardFork?.upstream?.repository || 'pubkey/rxdb',
  upstream_tag: sourcePackage.ctoxHardFork?.upstream?.version || sourcePackage.version,
  upstream_commit: sourcePackage.ctoxHardFork?.upstream?.commit || null,
  bundle_path: path.relative(repoRoot, bundlePath),
  bundle_sha256: createHash('sha256').update(bundle).digest('hex'),
  protocol: 'ctox-rxdb-protocol-v1',
  expected_exports: [
    'createRxDatabase',
    'getRxStorageDexie',
    'replicateWebRTC',
    'getConnectionHandlerSimplePeer',
    'RxDBMigrationSchemaPlugin',
    'removeRxDatabase',
    'addRxPlugin',
  ],
  status: 'built-from-ctox-rxdb-js-source',
};

if (writeProvenance || artifactDir) {
  await fs.writeFile(provenanceOutfile, `${JSON.stringify(provenance, null, 2)}\n`);
}

const result = spawnSync(process.execPath, [guardPath], {
  cwd: repoRoot,
  stdio: 'inherit',
});

if (result.status !== 0) {
  process.exit(result.status || 1);
}

console.log(`ctox-rxdb-js bundle provenance ${writeProvenance ? 'updated and verified' : 'verified'} (${provenance.bundle_sha256})`);

function readOption(name) {
  const index = process.argv.indexOf(name);
  if (index === -1) return '';
  const value = process.argv[index + 1];
  if (!value || value.startsWith('--')) {
    throw new Error(`${name} requires a value`);
  }
  return value;
}

async function buildBundle(entry, output) {
  const esbuild = await import(pathToFileURL(path.join(path.dirname(sourcePackagePath), 'node_modules', 'esbuild', 'lib', 'main.js')).href);
  await esbuild.build({
    entryPoints: [entry],
    outfile: output,
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    sourcemap: false,
    minify: false,
    logLevel: 'silent',
    mainFields: ['browser', 'module', 'main'],
    conditions: ['browser', 'import', 'default'],
    absWorkingDir: path.dirname(sourcePackagePath),
  });
}
