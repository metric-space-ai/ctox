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
  const minimumWidth = Number(presentation?.minimum_size?.width);
  const minimumHeight = Number(presentation?.minimum_size?.height);
  const layoutMinimumWidth = Number(manifest?.layout?.min_width);
  const layoutMinimumHeight = Number(manifest?.layout?.min_height);
  if (!Number.isInteger(minimumWidth) || minimumWidth < 360 || minimumWidth > Number(presentation?.initial_size?.width)) {
    failures.push(`${app.id}: minimum_size.width must be an integer between 360 and initial_size.width`);
  }
  // The promoted file-viewer retains its compact, multi-window preview contract.
  // Other modules still follow the original single-window migration contract.
  const isFileViewer = app.id === 'file-viewer';
  const minimumAllowedHeight = isFileViewer ? 400 : 480;
  if (!Number.isInteger(minimumHeight) || minimumHeight < minimumAllowedHeight || minimumHeight > Number(presentation?.initial_size?.height)) {
    failures.push(`${app.id}: minimum_size.height must be an integer between ${minimumAllowedHeight} and initial_size.height`);
  }
  if (isFileViewer && (minimumWidth !== 520 || minimumHeight !== 400 || manifest.launch_kind !== 'desktop-app')) {
    failures.push('file-viewer: promoted desktop preview must retain its 520x400 minimum and desktop-app launch kind');
  }
  if (Number.isFinite(layoutMinimumWidth) && layoutMinimumWidth !== minimumWidth) {
    failures.push(`${app.id}: layout.min_width must match presentation.minimum_size.width when declared`);
  }
  if (Number.isFinite(layoutMinimumHeight) && layoutMinimumHeight !== minimumHeight) {
    failures.push(`${app.id}: layout.min_height must match presentation.minimum_size.height when declared`);
  }
  if (presentation.multi_instance !== isFileViewer) failures.push(`${app.id}: multi_instance must be ${isFileViewer} for its registered presentation contract`);
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
