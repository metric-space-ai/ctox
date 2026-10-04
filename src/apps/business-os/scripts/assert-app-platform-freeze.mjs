import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const businessOsRoot = join(here, '..');
const modulesRoot = join(businessOsRoot, 'modules');

// Phase-0 freeze: these are existing migration debts, not approved patterns.
// Entries may be removed without changing this list. A new path fails the gate.
const LEGACY_CONTEXTMENU_FILES = new Set([
  'modules/browser/index.js',
  'modules/desktop/index.js',
]);

// Source-only workbenches are not part of the canonical module registry until
// they gain a reviewed module.json. Keep this explicit so an accidental new
// manifest-less app directory still fails the freeze.
const SOURCE_ONLY_MODULE_DIRS = new Set([
  'creator',
]);

// Existing full-workspace modules remain valid during the compatibility
// migration. New modules must use the Presentation/Windowed contract instead.
const LEGACY_FULL_WORKSPACE_MODULES = new Set([
  'desktop',
]);

const SHELL_SURFACE_MODULES = new Set(['desktop']);

const CONTEXTMENU_HANDLER = /(?:addEventListener\s*\(\s*['"]contextmenu['"]|\.oncontextmenu\s*=)/;
// What the freeze forbids is a module building its own menu, not a module
// opening the shell's. A handler that routes through the host-provided
// `ctx.contextMenu` is the approved contract, so it is not a debt — the plain
// handler grep flagged `modules/explorer/index.js` for following it and left CI
// red on main (found 09.09.2026).
const SHELL_CONTEXTMENU_DELEGATION = /(?:ctx\.contextMenu|contextMenu\.show\s*\(|createContextMenu\s*\()/;
const offenders = [];
const observedLegacyMenus = [];

for (const file of walk(modulesRoot)) {
  if (!file.endsWith('.js')) continue;
  const source = readFileSync(file, 'utf8');
  if (!CONTEXTMENU_HANDLER.test(source)) continue;
  const rel = relative(businessOsRoot, file).replaceAll('\\', '/');
  if (LEGACY_CONTEXTMENU_FILES.has(rel)) {
    observedLegacyMenus.push(rel);
    continue;
  }
  if (SHELL_CONTEXTMENU_DELEGATION.test(source)) continue;
  offenders.push(`new module-local contextmenu handler: ${rel}`);
}

for (const entry of readdirSync(modulesRoot, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const manifestPath = join(modulesRoot, entry.name, 'module.json');
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  } catch (error) {
    if (error?.code === 'ENOENT' && SOURCE_ONLY_MODULE_DIRS.has(entry.name)) continue;
    offenders.push(`cannot inspect ${relative(businessOsRoot, manifestPath)}: ${error.message}`);
    continue;
  }
  if (manifest?.layout?.shell === 'full-workspace'
      && !LEGACY_FULL_WORKSPACE_MODULES.has(entry.name)) {
    offenders.push(`new full-workspace-only module: modules/${entry.name}/module.json`);
  }
  if (!SHELL_SURFACE_MODULES.has(entry.name)) {
    if (manifest?.launch_kind !== 'desktop-app') {
      offenders.push(`canonical windowed module lacks root launch_kind: modules/${entry.name}/module.json`);
    }
    if (manifest?.layout?.launch_kind != null) {
      offenders.push(`deprecated duplicate layout.launch_kind: modules/${entry.name}/module.json`);
    }
    if (!['window', 'focus'].includes(manifest?.presentation?.default_mode)
        || !manifest.presentation?.supported_modes?.includes?.('window')
        || !manifest.presentation?.supported_modes?.includes?.('maximized')
        || !manifest.presentation?.supported_modes?.includes?.('focus')) {
      offenders.push(`canonical presentation contract missing: modules/${entry.name}/module.json`);
    }
    if ((manifest?.presentation?.minimum_size?.width || Infinity) > 640
        || (manifest?.presentation?.minimum_size?.height || Infinity) > 480) {
      offenders.push(`windowed module is not responsive to 640x480: modules/${entry.name}/module.json`);
    }
  }
}

if (offenders.length) {
  console.error('Business OS app-platform phase-0 freeze failed:');
  for (const offender of offenders) console.error(`- ${offender}`);
  process.exit(1);
}

console.log(
  `Business OS app-platform freeze OK (${observedLegacyMenus.length}/${LEGACY_CONTEXTMENU_FILES.size} legacy contextmenu files remain; no new full-workspace modules)`,
);

function walk(root) {
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === 'vendor') continue;
      files.push(...walk(path));
    } else if (entry.isFile() || statSync(path).isFile()) {
      files.push(path);
    }
  }
  return files;
}
