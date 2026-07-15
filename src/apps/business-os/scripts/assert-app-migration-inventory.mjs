#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadBusinessOsAppInventory } from './business-os-app-inventory.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const modulesRoot = path.join(repoRoot, 'src/apps/business-os/modules');
const inventory = loadBusinessOsAppInventory();
const failures = [];
const shellSource = fs.readFileSync(path.join(repoRoot, 'src/apps/business-os/app.js'), 'utf8');

for (const marker of [
  'applyWindowedLoadingShadow(mod, content, loadingToken)',
  'class="module-loading-shadow is-pending" data-loading-shadow',
  "content.querySelector('[data-loading-shadow]')?.remove()",
]) {
  if (!shellSource.includes(marker)) failures.push(`shell: missing windowed loading-shadow invariant ${JSON.stringify(marker)}`);
}

for (const app of inventory.sourceApps) {
  const moduleRoot = path.join(modulesRoot, app.id);
  const manifest = JSON.parse(fs.readFileSync(path.join(moduleRoot, 'module.json'), 'utf8'));
  for (const required of ['index.html', 'index.css', 'index.js', 'module.json']) {
    if (!fs.existsSync(path.join(moduleRoot, required))) {
      failures.push(`${app.id}: missing ${required}`);
    }
  }

  if (app.kind === 'shell-surface') {
    if (manifest?.layout?.shell !== 'full-workspace') {
      failures.push(`${app.id}: shell surface must remain layout.shell=full-workspace`);
    }
    continue;
  }

  const presentation = manifest?.presentation;
  if (manifest?.layout?.shell !== 'windowed') failures.push(`${app.id}: layout.shell must be windowed`);
  if (!presentation || typeof presentation !== 'object') {
    failures.push(`${app.id}: missing canonical presentation contract`);
    continue;
  }
  const supportedModes = Array.isArray(presentation.supported_modes) ? presentation.supported_modes : [];
  for (const mode of ['window', 'maximized']) {
    if (!supportedModes.includes(mode)) failures.push(`${app.id}: presentation.supported_modes missing ${mode}`);
  }
  if (!['window', 'maximized', 'focus'].includes(presentation.default_mode)) {
    failures.push(`${app.id}: invalid presentation.default_mode ${JSON.stringify(presentation.default_mode)}`);
  }
  if (presentation?.minimum_size?.width !== 640 || presentation?.minimum_size?.height !== 480) {
    failures.push(`${app.id}: minimum_size must be exactly 640x480`);
  }
  if (presentation.multi_instance !== false) failures.push(`${app.id}: multi_instance must be false in migration v1`);
  if (presentation.auto_restore !== false) failures.push(`${app.id}: auto_restore must be false in migration v1`);
}

if (failures.length) {
  console.error(`Business OS app migration inventory failed (${failures.length}):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`business_os_source_app_inventory=${inventory.sourceApps.length}`);
console.log(`business_os_system_app_inventory=${inventory.coreApps.length}`);
console.log(`business_os_compatibility_surface_inventory=${inventory.compatibilityApps.length}`);
console.log('business_os_app_migration_inventory_ok=1');
