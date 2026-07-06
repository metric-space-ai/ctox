// GUARD: dist/ctox-rxdb-js.mjs must be reproducible from src/.
//
// The single most damaging regression pattern in this runtime's history was
// src<->dist drift: agents patching the built bundle directly (so the next
// src build silently REVERTED their fix), or patching src without rebuilding
// (so the browser never received the fix). Both directions shipped real
// production breakage.
//
// This guard rebuilds the bundle from src/index.mjs with the pinned esbuild
// and fails if the result differs from the committed dist file.
//
// If this test fails you (the agent reading this) must NOT edit dist to make
// it pass. Fix src, then rebuild dist with EXACTLY:
//
//   npx -y esbuild@0.28.0 src/apps/business-os/rxdb/src/index.mjs \
//     --bundle --format=esm \
//     --outfile=src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs \
//     "--banner:js=// CTOX Sync Engine app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs."
//
// and bump the cache-buster (`ctox-rxdb-js.mjs?v=...`) in shared/db.js,
// shared/sync.js and modules/matching/ui/businessOsDataSource.js.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ESBUILD_PIN = 'esbuild@0.28.0';
const BANNER = '// CTOX Sync Engine app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs.';

const testDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(testDir, '..');
const entry = resolve(appRoot, 'src/index.mjs');
const distPath = resolve(appRoot, 'dist/ctox-rxdb-js.mjs');

const workDir = mkdtempSync(join(tmpdir(), 'ctox-rxdb-bundle-guard-'));
const outfile = join(workDir, 'ctox-rxdb-js.mjs');

try {
  try {
    execFileSync('npx', ['-y', ESBUILD_PIN, entry, '--bundle', '--format=esm', `--outfile=${outfile}`, `--banner:js=${BANNER}`], {
      stdio: 'pipe',
      timeout: 180_000,
    });
  } catch (error) {
    // Offline / npx unavailable: skip LOUDLY (exit 0 so air-gapped dev works),
    // but never skip silently — CI has network and will enforce this.
    console.error('bundle-reproducible-smoke SKIPPED: could not run pinned esbuild via npx.');
    console.error(String(error?.message || error).slice(0, 300));
    console.log('ctox-rxdb bundle reproducibility guard SKIPPED (esbuild unavailable)');
    process.exit(0);
  }

  const rebuilt = readFileSync(outfile, 'utf8');
  const committed = readFileSync(distPath, 'utf8');
  if (rebuilt !== committed) {
    const rebuiltLines = rebuilt.split('\n');
    const committedLines = committed.split('\n');
    let firstDiff = 0;
    while (
      firstDiff < Math.min(rebuiltLines.length, committedLines.length)
      && rebuiltLines[firstDiff] === committedLines[firstDiff]
    ) firstDiff += 1;
    console.error('BUNDLE DRIFT: dist/ctox-rxdb-js.mjs does not match a rebuild from src/.');
    console.error(`dist lines: ${committedLines.length}, rebuild lines: ${rebuiltLines.length}, first differing line: ${firstDiff + 1}`);
    console.error(`  dist:    ${String(committedLines[firstDiff] ?? '<EOF>').slice(0, 160)}`);
    console.error(`  rebuild: ${String(rebuiltLines[firstDiff] ?? '<EOF>').slice(0, 160)}`);
    console.error('Either src was changed without rebuilding dist, or dist was patched directly.');
    console.error('Fix src and rebuild dist with the pinned command in this file’s header.');
    process.exit(1);
  }
} finally {
  rmSync(workDir, { recursive: true, force: true });
}

console.log('ctox-rxdb bundle reproducibility guard OK');
