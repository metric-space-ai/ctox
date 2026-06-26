import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const scriptDir = import.meta.dirname;
const srcRoot = path.resolve(scriptDir, '..', '..');
const repoRoot = path.resolve(srcRoot, '..');
const businessOsRoot = path.join(srcRoot, 'apps', 'business-os');
const vendorRoot = path.join(businessOsRoot, 'vendor');
const archivedWordPortRoot = path.join(
  repoRoot,
  'archive',
  'reorg-review',
  'templates',
  'business-basic',
  'apps',
  'web',
  'lib',
  'word-port',
);
const archivedGeneratedNodeModules = path.join(
  repoRoot,
  'archive',
  '2026-05-18-cleanup',
  'generated',
  'templates',
  'business-basic',
  'node_modules',
);

const esbuild = await loadEsbuild();

await buildDocumentFormat();
await buildSuperdoc();
await buildHyperFormula();
await buildLexical();

async function buildDocumentFormat() {
  const entry = path.join(businessOsRoot, 'modules', 'documents', 'document-format', 'src', 'index.ts');
  const outfile = path.join(vendorRoot, 'document-format.mjs');
  await esbuild.build({
    entryPoints: [entry],
    outfile,
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    sourcemap: false,
    minify: false,
    logLevel: 'info',
    mainFields: ['browser', 'module', 'main'],
    conditions: ['browser', 'import', 'default'],
    nodePaths: [archivedGeneratedNodeModules],
    plugins: [wordPortArchiveResolver()],
  });
  await report(outfile);
}

async function buildSuperdoc() {
  const entry = path.join(vendorRoot, 'superdoc', 'superdoc.es.js');
  const outfile = path.join(vendorRoot, 'superdoc.mjs');
  if (!(await fileExists(entry))) {
    if (await fileExists(outfile)) {
      console.log(`kept ${path.relative(repoRoot, outfile)} (source chunks are not vendored)`);
      return;
    }
    throw new Error(`SuperDoc source bundle is missing: ${entry}`);
  }
  await esbuild.build({
    entryPoints: [entry],
    outfile,
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    sourcemap: false,
    minify: false,
    logLevel: 'info',
    mainFields: ['browser', 'module', 'main'],
    conditions: ['browser', 'import', 'default'],
  });
  await report(outfile);
}

async function buildHyperFormula() {
  const entry = path.join(vendorRoot, 'hyperformula', 'HyperFormula.js');
  const outfile = path.join(vendorRoot, 'hyperformula.mjs');
  await esbuild.build({
    entryPoints: [entry],
    outfile,
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    sourcemap: false,
    minify: false,
    logLevel: 'info',
  });
  await report(outfile);
}

async function buildLexical() {
  const entry = path.join(businessOsRoot, 'vendor', 'lexical-src', 'index.js');
  const outfile = path.join(vendorRoot, 'lexical.mjs');
  await esbuild.build({
    entryPoints: [entry],
    outfile,
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    sourcemap: false,
    minify: false,
    logLevel: 'info',
    mainFields: ['browser', 'module', 'main'],
    conditions: ['browser', 'import', 'default'],
    nodePaths: [archivedGeneratedNodeModules],
  });
  await report(outfile);
}

async function fileExists(file) {
  try {
    await fs.access(file);
    return true;
  } catch {
    return false;
  }
}

function wordPortArchiveResolver() {
  const wordPortEntry = path.join(archivedWordPortRoot, 'index.ts');
  const packageAliases = new Map([
    ['fast-xml-parser', path.join(archivedGeneratedNodeModules, '.pnpm', 'fast-xml-parser@5.8.0', 'node_modules', 'fast-xml-parser', 'lib', 'fxp.cjs')],
    ['jszip', path.join(archivedGeneratedNodeModules, '.pnpm', 'jszip@3.10.1', 'node_modules', 'jszip', 'dist', 'jszip.min.js')],
  ]);
  return {
    name: 'word-port-archive-resolver',
    setup(build) {
      build.onResolve({ filter: /^@ctox-word-port-archive$/ }, () => ({
        path: wordPortEntry,
      }));
      build.onResolve({ filter: /^(fast-xml-parser|jszip)$/ }, (args) => ({
        path: packageAliases.get(args.path),
      }));
    },
  };
}

async function report(file) {
  const stats = await fs.stat(file);
  console.log(`built ${path.relative(repoRoot, file)} (${stats.size} bytes)`);
}

async function loadEsbuild() {
  try {
    return await import('esbuild');
  } catch {}

  const pnpmRoots = [
    path.join(repoRoot, 'archive', '2026-05-18-cleanup', 'generated', 'templates', 'business-basic', 'node_modules', '.pnpm'),
  ];

  for (const pnpmRoot of pnpmRoots) {
    let entries = [];
    try {
      entries = await fs.readdir(pnpmRoot);
    } catch {
      continue;
    }
    const matches = entries.filter((entry) => entry.startsWith('esbuild@')).sort(comparePackageVersions).reverse();
    for (const match of matches) {
      const modulePath = path.join(pnpmRoot, match, 'node_modules', 'esbuild', 'lib', 'main.js');
      try {
        return await import(pathToFileURL(modulePath).href);
      } catch {}
    }
  }

  throw new Error('esbuild is not available. Install it or restore the archived generated node_modules bundle first.');
}

function comparePackageVersions(left, right) {
  const leftVersion = left.replace(/^esbuild@/, '').split('.').map(Number);
  const rightVersion = right.replace(/^esbuild@/, '').split('.').map(Number);
  for (let i = 0; i < Math.max(leftVersion.length, rightVersion.length); i += 1) {
    const delta = (leftVersion[i] || 0) - (rightVersion[i] || 0);
    if (delta) return delta;
  }
  return left.localeCompare(right);
}
