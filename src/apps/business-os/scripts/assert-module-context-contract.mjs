// Backlog OS-D1: pin the `mount(ctx)` module-context contract.
//
// The ctx object built in app.js::createModuleContext is the platform API
// every Business OS module — and every agent-generated app — programs
// against. This guard extracts the top-level field list from the marked
// literal (CTX-CONTRACT-BEGIN/END markers) and compares it against the
// pinned contract below, which mirrors docs/business-os-module-context.md.
//
//  - A field present in app.js but missing here: the contract doc was not
//    updated — add the field to BOTH in the same change.
//  - A field pinned here but missing in app.js: a module-facing API was
//    removed or renamed. That is a BREAKING module-API change; it needs an
//    explicit decision and a contract version bump, not a drive-by edit.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const appJsPath = resolve(here, '..', 'app.js');

const CONTRACT_VERSION = 'business-os-module-context-v1';

// Keep sorted. Mirrors docs/business-os-module-context.md.
const CONTRACT_FIELDS = [
  'actor',
  'businessChat',
  'canModifyModule',
  'closeDrawers',
  'commandBus',
  'contextMenu',
  'db',
  'desktopApps',
  'eventBus',
  'getDesktopApps',
  'getModules',
  'getSvgIcon',
  'governance',
  'host',
  'isTaskbarPinned',
  'left',
  'locale',
  'module',
  'modules',
  'notifications',
  'openBottomDrawer',
  'openBusinessChat',
  'openDesktopApp',
  'openLeftDrawer',
  'openRightDrawer',
  'permissions',
  'pinToTaskbar',
  'presence',
  'reportFileIntegrityError',
  'reportIssue',
  'right',
  'runtimeCapabilities',
  'session',
  'shellStyle',
  'storageScope',
  'sync',
  'syncConfig',
  'toggleTaskbarPin',
  'unpinFromTaskbar',
  'user',
  'windowManager',
];

const source = readFileSync(appJsPath, 'utf8');
const beginMarker = `// CTX-CONTRACT-BEGIN ${CONTRACT_VERSION}`;
const endMarker = `// CTX-CONTRACT-END ${CONTRACT_VERSION}`;
const beginIndex = source.indexOf(beginMarker);
const endIndex = source.indexOf(endMarker);
if (beginIndex < 0 || endIndex < 0 || endIndex <= beginIndex) {
  fail(`ctx contract markers (${CONTRACT_VERSION}) not found in app.js — they are load-bearing for this guard`);
}
const literal = source.slice(beginIndex, endIndex);

// Top-level keys sit at exactly 4 spaces of indentation inside the returned
// literal; nested object fields (user, etc.) are indented deeper.
const foundFields = [...literal.matchAll(/^ {4}([A-Za-z_$][\w$]*)\s*[:,(]/gm)]
  .map((match) => match[1])
  .filter((name) => name !== 'return');
const found = [...new Set(foundFields)].sort();
const pinned = [...CONTRACT_FIELDS].sort();

const missingFromContract = found.filter((name) => !pinned.includes(name));
const missingFromCtx = pinned.filter((name) => !found.includes(name));

if (missingFromContract.length || missingFromCtx.length) {
  const lines = ['module-context contract drift detected:'];
  if (missingFromContract.length) {
    lines.push(
      `  fields in app.js but NOT in the pinned contract (update the pin AND docs/business-os-module-context.md): ${missingFromContract.join(', ')}`,
    );
  }
  if (missingFromCtx.length) {
    lines.push(
      `  fields pinned but MISSING from app.js (breaking module-API change — needs an explicit decision): ${missingFromCtx.join(', ')}`,
    );
  }
  fail(lines.join('\n'));
}

console.log(`module-context contract OK (${CONTRACT_VERSION}, ${pinned.length} fields)`);

function fail(message) {
  console.error(message);
  process.exit(1);
}
