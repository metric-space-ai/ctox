import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';

const scriptDir = import.meta.dirname;
const srcRoot = path.resolve(scriptDir, '..', '..');
const repoRoot = path.resolve(srcRoot, '..');
const appLocalRoot = path.join(srcRoot, 'apps', 'business-os', 'rxdb');
const sourceBundlePath = path.join(appLocalRoot, 'dist', 'ctox-rxdb-js.mjs');
const manifestPath = path.join(appLocalRoot, 'manifest.json');
const readmePath = path.join(appLocalRoot, 'README.md');
const srcDir = path.join(appLocalRoot, 'src');
const testsDir = path.join(appLocalRoot, 'tests');
const buildDir = path.join(repoRoot, 'runtime', 'build', 'ctox-rxdb-js');
const artifactDir = readOption('--artifact-dir');
const outDir = artifactDir ? path.resolve(repoRoot, artifactDir) : buildDir;
const outfile = path.join(outDir, 'ctox-rxdb-js.mjs');
const provenanceOutfile = path.join(outDir, 'ctox-rxdb-js.provenance.json');
const writeProvenance = process.argv.includes('--write-provenance');
const writeBundle = process.argv.includes('--write-bundle');

const manifest = JSON.parse(await fs.readFile(manifestPath, 'utf8'));
if (manifest.name !== 'ctox-rxdb-js' || manifest.public_name !== 'CTOX DB') {
  throw new Error('app-local CTOX DB manifest identity is invalid');
}
if (manifest.package_manager !== 'none') {
  throw new Error('app-local CTOX DB must remain package-manager-free');
}

await fs.mkdir(outDir, { recursive: true });
if (writeBundle || artifactDir) {
  await fs.copyFile(sourceBundlePath, outfile);
}

const bundle = await fs.readFile(sourceBundlePath);
const provenance = {
  name: 'ctox-db-browser-runtime',
  public_name: manifest.public_name,
  runtime_id: manifest.name,
  api_contract: manifest.api_contract,
  compatibility: manifest.compatibility,
  upstream_compatible: manifest.upstream_compatible,
  upstream_compatibility: manifest.upstream_compatibility,
  package_manager: manifest.package_manager,
  protocol: manifest.protocol,
  bundle_path: relative(sourceBundlePath),
  bundle_sha256: sha256Buffer(bundle),
  manifest_path: relative(manifestPath),
  manifest_sha256: await sha256File(manifestPath),
  readme_path: relative(readmePath),
  readme_sha256: await sha256File(readmePath),
  source_hashes: await hashFiles(srcDir, '.mjs'),
  test_hashes: await hashFiles(testsDir, '.mjs'),
  status: 'app-local-ctox-db-runtime',
};

if (writeProvenance || artifactDir) {
  await fs.writeFile(provenanceOutfile, `${JSON.stringify(provenance, null, 2)}\n`);
}

console.log(`ctox-db app-local runtime evidence ${writeProvenance || artifactDir ? 'written' : 'verified'} (${provenance.bundle_sha256})`);

function readOption(name) {
  const index = process.argv.indexOf(name);
  if (index === -1) return '';
  const value = process.argv[index + 1];
  if (!value || value.startsWith('--')) {
    throw new Error(`${name} requires a value`);
  }
  return value;
}

async function hashFiles(dir, extension) {
  const files = (await fs.readdir(dir, { withFileTypes: true }))
    .filter((entry) => entry.isFile() && entry.name.endsWith(extension))
    .map((entry) => path.join(dir, entry.name))
    .sort();
  const out = {};
  for (const file of files) {
    out[relative(file)] = await sha256File(file);
  }
  return out;
}

async function sha256File(file) {
  return sha256Buffer(await fs.readFile(file));
}

function sha256Buffer(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function relative(file) {
  return path.relative(repoRoot, file);
}
