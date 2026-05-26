#!/usr/bin/env node
// Bundles src/index.mjs into dist/ctox-rxdb-js.mjs via esbuild.
// Run: node src/apps/business-os/rxdb/tools/build-dist.mjs

import { build } from 'esbuild';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const entry = resolve(root, 'src/index.mjs');
const outfile = resolve(root, 'dist/ctox-rxdb-js.mjs');

await build({
  entryPoints: [entry],
  outfile,
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: 'es2022',
  sourcemap: false,
  minify: false,
  logLevel: 'info',
  // node:zlib is conditionally imported only when DecompressionStream is
  // unavailable (i.e. test runs in Node). The browser bundle never reaches
  // that branch but esbuild still wants to resolve the import.
  external: ['node:zlib'],
});
console.log(`built ${outfile}`);
