import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import {
  createShellGenerationReloadGuard,
  isShellGenerationMismatchResponse,
} from './shell-generation.js';

function response(status, headers = {}) {
  const normalized = new Map(Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]));
  return {
    status,
    headers: {
      get(name) {
        return normalized.get(String(name).toLowerCase()) || null;
      },
    },
  };
}

const mismatch = () => response(409, {
  'x-ctox-shell-generation-mismatch': '1',
  'x-ctox-shell-generation': '20260831-shell-v2-atomic-v304',
});

test('index, app and generation-bound imports declare one shell generation', async () => {
  const [appSource, indexSource] = await Promise.all([
    readFile(new URL('../app.js', import.meta.url), 'utf8'),
    readFile(new URL('../index.html', import.meta.url), 'utf8'),
  ]);
  const active = appSource.match(/const APP_BUILD = ['"]([^'"]+)['"]/)?.[1] || '';
  assert.match(active, /-shell-v2-/);

  const appGenerationTokens = [...appSource.matchAll(/\?v=([^'"`\s]*-shell-v2-[^'"`\s]*)/g)]
    .map((match) => match[1]);
  assert.ok(appGenerationTokens.length > 0);
  assert.deepEqual([...new Set(appGenerationTokens)], [active]);

  for (const asset of [
    'app.css',
    'shared/base.css',
    'mobile-host.css',
    'themes/ctox-desktop-shell.css',
    'shared/shell-release-status.js',
    'mobile-host.js',
    'app.js',
  ]) {
    assert.match(indexSource, new RegExp(`${asset.replaceAll('.', '\\.') }\\?v=${active}`));
  }
});

test('shell startup and retry preserve local replicas until server acknowledgment is verified', async () => {
  const appSource = await readFile(new URL('../app.js', import.meta.url), 'utf8');
  assert.match(appSource, /await reportLegacySharedBusinessDb\(syncConfig\)/);
  assert.match(appSource, /await reportSupersededBusinessDbGenerations\(syncConfig\)/);
  assert.doesNotMatch(appSource, /\bresetBusinessDb\s*\(|\bindexedDB\.deleteDatabase\s*\(/);
  assert.doesNotMatch(appSource, /resetLocalRxDbBeforeStartupRetry|purgeSupersededBusinessDbGenerations/);
});

test('only the explicit server mismatch contract triggers recovery', () => {
  assert.equal(isShellGenerationMismatchResponse(response(200)), false);
  assert.equal(isShellGenerationMismatchResponse(response(409)), false);
  assert.equal(isShellGenerationMismatchResponse(mismatch()), true);
});

test('a mismatch schedules exactly one controlled reload for the active generation', () => {
  const markers = new Map();
  const deferred = [];
  let reloads = 0;
  const guard = createShellGenerationReloadGuard({
    readMarker: (key) => markers.get(key) || null,
    writeMarker: (key, value) => markers.set(key, value),
    defer: (callback) => deferred.push(callback),
    reload: () => { reloads += 1; },
  });

  assert.equal(guard.inspect(mismatch()), true);
  assert.equal(guard.scheduled, true);
  assert.equal(deferred.length, 1);
  assert.equal(guard.inspect(mismatch()), false);
  assert.equal(deferred.length, 1);
  deferred[0]();
  assert.equal(reloads, 1);
  assert.equal(markers.get('ctox.businessOs.shellGenerationReload.20260831-shell-v2-atomic-v304'), '1');
});

test('the session marker prevents a reload loop while still failing closed', () => {
  let reloads = 0;
  const guard = createShellGenerationReloadGuard({
    readMarker: () => '1',
    writeMarker: () => assert.fail('existing marker must not be rewritten'),
    defer: (callback) => callback(),
    reload: () => { reloads += 1; },
  });

  assert.equal(guard.inspect(mismatch()), true);
  assert.equal(guard.scheduled, true);
  assert.equal(reloads, 0);
});

test('restricted session storage still permits one recovery reload', () => {
  let reloads = 0;
  const guard = createShellGenerationReloadGuard({
    readMarker: () => { throw new Error('storage denied'); },
    defer: (callback) => callback(),
    reload: () => { reloads += 1; },
  });

  assert.equal(guard.inspect(mismatch()), true);
  assert.equal(reloads, 1);
});
