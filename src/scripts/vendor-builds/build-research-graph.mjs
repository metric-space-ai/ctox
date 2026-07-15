#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { execFile as execFileCallback } from 'node:child_process';
import { mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';

const execFile = promisify(execFileCallback);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..', '..', '..');
const businessOsRoot = path.join(repoRoot, 'src', 'apps', 'business-os');
const vendorRoot = path.join(businessOsRoot, 'vendor');
const sourceRoot = path.join(repoRoot, 'runtime', 'vendor-sources', 'research-graph');
const inputPath = path.join(scriptDir, 'research-graph-packages.json');
const bundlePath = path.join(vendorRoot, 'research-graph.mjs');
const provenancePath = path.join(vendorRoot, 'research-graph.provenance.json');
const licensesPath = path.join(vendorRoot, 'research-graph.LICENSES.txt');
const install = process.argv.includes('--install');
const input = JSON.parse(await readFile(inputPath, 'utf8'));
const packageSpecs = Object.entries(input.packages).map(([name, version]) => `${name}@${version}`);

if (install) {
  await rm(sourceRoot, { recursive: true, force: true });
  await mkdir(sourceRoot, { recursive: true });
  await writeFile(path.join(sourceRoot, 'package.json'), `${JSON.stringify({
    name: 'ctox-research-graph-vendor-source',
    private: true,
    type: 'module',
    dependencies: input.packages,
  }, null, 2)}\n`);
  await execFile('npm', [
    'install',
    '--prefix', sourceRoot,
    '--ignore-scripts',
    '--no-audit',
    '--no-fund',
    '--save-exact',
    ...packageSpecs,
  ], { maxBuffer: 32 * 1024 * 1024 });
}

const lockPath = path.join(sourceRoot, 'package-lock.json');
const lock = JSON.parse(await readFile(lockPath, 'utf8').catch(() => {
  throw new Error(`Pinned Research Graph source closure is missing. Run ${path.relative(repoRoot, fileURLToPath(import.meta.url))} --install.`);
}));

for (const [name, version] of Object.entries(input.packages)) {
  const packageJsonPath = path.join(sourceRoot, 'node_modules', name, 'package.json');
  const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf8'));
  if (packageJson.version !== version) {
    throw new Error(`${name} version mismatch: expected ${version}, got ${packageJson.version}`);
  }
}

const esbuildPath = path.join(businessOsRoot, 'node_modules', 'esbuild', 'lib', 'main.js');
const esbuild = await import(pathToFileURL(esbuildPath).href);
await mkdir(vendorRoot, { recursive: true });
await esbuild.build({
  stdin: {
    contents: [
      "import ForceGraph3D from '3d-force-graph';",
      "import SpriteText from 'three-spritetext';",
      "export { AdditiveBlending, BoxGeometry, CanvasTexture, Color, Group, Mesh, MeshLambertMaterial, SphereGeometry, Sprite, SpriteMaterial, Vector3 } from 'three';",
      'export { ForceGraph3D, SpriteText };',
    ].join('\n'),
    resolveDir: sourceRoot,
    sourcefile: 'ctox-research-graph-entry.mjs',
    loader: 'js',
  },
  outfile: bundlePath,
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: input.target,
  minify: true,
  sourcemap: false,
  legalComments: 'eof',
  logLevel: 'info',
  mainFields: ['browser', 'module', 'main'],
  conditions: ['browser', 'import', 'default'],
  nodePaths: [path.join(sourceRoot, 'node_modules')],
});

const packages = await packageInventory(lock);
await writeFile(licensesPath, await licenseInventoryText(packages));
const bundleBytes = await readFile(bundlePath);
const provenance = {
  schema_version: 'ctox-research-graph-vendor-provenance-v1',
  generator: 'src/scripts/vendor-builds/build-research-graph.mjs',
  input: 'src/scripts/vendor-builds/research-graph-packages.json',
  runtime_format: input.runtime_format,
  target: input.target,
  runtime_package_manager: 'none',
  entry_packages: input.packages,
  package_lock_sha256: sha256(await readFile(lockPath)),
  packages,
  outputs: [
    {
      path: relative(bundlePath),
      bytes: bundleBytes.length,
      sha256: sha256(bundleBytes),
    },
    {
      path: relative(licensesPath),
      bytes: (await stat(licensesPath)).size,
      sha256: sha256(await readFile(licensesPath)),
    },
  ],
};
await writeFile(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`);
console.log(`built ${relative(bundlePath)} (${bundleBytes.length} bytes, ${packages.length} pinned packages)`);

async function packageInventory(packageLock) {
  const result = [];
  for (const [lockKey, lockEntry] of Object.entries(packageLock.packages || {})) {
    if (!lockKey.includes('node_modules/')) continue;
    const packageRoot = path.resolve(sourceRoot, lockKey);
    const packageJson = JSON.parse(await readFile(path.join(packageRoot, 'package.json'), 'utf8'));
    result.push({
      name: packageJson.name,
      version: packageJson.version,
      license: packageJson.license || lockEntry.license || 'UNKNOWN',
      resolved: lockEntry.resolved || '',
      integrity: lockEntry.integrity || '',
      source: packageJson.repository?.url || packageJson.homepage || '',
      license_file: await findLicenseFile(packageRoot),
      package_path: `node_modules/${packageJson.name}`,
    });
  }
  return result.sort((left, right) => left.name.localeCompare(right.name));
}

async function findLicenseFile(packageRoot) {
  const files = await readdir(packageRoot);
  const name = files.find((file) => /^(license|licence|copying)(\.|$)/i.test(file));
  return name || '';
}

async function licenseInventoryText(packages) {
  const sections = [
    'CTOX Research Graph vendored dependency licenses',
    'Generated by src/scripts/vendor-builds/build-research-graph.mjs.',
    '',
  ];
  for (const item of packages) {
    sections.push(`${item.name}@${item.version} (${item.license})`);
    sections.push(item.source || item.resolved || 'Source recorded in provenance JSON.');
    if (item.license_file) {
      sections.push('');
      sections.push((await readFile(path.join(sourceRoot, item.package_path, item.license_file), 'utf8')).trim());
    } else {
      sections.push('Full license identifier is recorded in package metadata; no standalone license file was distributed in the package.');
    }
    sections.push('', '-------------------------------------------------------------------------------', '');
  }
  return `${sections.join('\n')}\n`;
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function relative(file) {
  return path.relative(repoRoot, file).replaceAll('\\', '/');
}
