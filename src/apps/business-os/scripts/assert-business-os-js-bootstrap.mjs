#!/usr/bin/env node
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';

const packageUrl = new URL('../package.json', import.meta.url);
const lockUrl = new URL('../package-lock.json', import.meta.url);
const requireFromBusinessOs = createRequire(packageUrl);

const expectedDevDependencies = {
  esbuild: '0.28.1',
  playwright: '1.60.0',
};

const packageJson = JSON.parse(await readFile(packageUrl, 'utf8'));
const lockJson = JSON.parse(await readFile(lockUrl, 'utf8'));

assert.equal(packageJson.private, true, 'Business OS test bootstrap package must stay private');

for (const [name, version] of Object.entries(expectedDevDependencies)) {
  assert.equal(
    packageJson.devDependencies?.[name],
    version,
    `package.json must pin ${name}@${version}`,
  );
  assert.equal(
    lockJson.packages?.['']?.devDependencies?.[name],
    version,
    `package-lock.json root must pin ${name}@${version}`,
  );
  const resolved = requireFromBusinessOs.resolve(name);
  assert.ok(resolved.includes(`/node_modules/${name}/`), `${name} must resolve from Business OS node_modules`);
}

const esbuild = requireFromBusinessOs('esbuild');
const playwright = requireFromBusinessOs('playwright');

assert.equal(typeof esbuild.build, 'function', 'esbuild build API must be available');
assert.equal(typeof playwright.chromium?.launch, 'function', 'Playwright chromium API must be available');

console.log('business_os_js_bootstrap=1');
console.log(`business_os_js_bootstrap_esbuild=${expectedDevDependencies.esbuild}`);
console.log(`business_os_js_bootstrap_playwright=${expectedDevDependencies.playwright}`);
