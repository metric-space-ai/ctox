import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import {
  appIdentityFromSource,
  buildAppImportCommand,
  confirmSnapshotDocuments,
  isImportableFile,
  importFileByteLimit,
  isRecoverableDispatchError,
  isTextFile,
  moduleIdFromSource,
  normalizeImportCategory,
  parseGitHubUrl,
  shouldSkipPath,
  standaloneHtmlEntryPath,
  validModuleId,
} from './index.js';

test('snapshot upload retries targeted pushes until the native peer acknowledges every row', async (t) => {
  // Model protocol time explicitly so concurrent browser tests cannot consume the retry budget.
  let now = 0;
  t.mock.method(Date, 'now', () => now);
  const releases = [];
  let attempts = 0;
  const ctx = {
    sync: {
      async leaseCollection(collection, reason) {
        assert.equal(collection, 'desktop_file_chunks');
        assert.equal(reason, 'app-import-snapshot:desktop_file_chunks');
        const attempt = ++attempts;
        now += 5;
        return {
          bridge: {
            state: {
              async pushDocumentsToRemotePeers(rows) {
                assert.deepEqual(rows.map((row) => row.id), ['chunk-1', 'chunk-2']);
                if (attempt === 1) throw new Error('peer reconnected during push');
                return true;
              },
            },
          },
          async release() { releases.push(attempt); },
        };
      },
    },
  };
  await confirmSnapshotDocuments(ctx, 'desktop_file_chunks', [
    { id: 'chunk-1' }, { id: 'chunk-2' },
  ], { timeoutMs: 100, retryMs: 0 });
  assert.equal(attempts, 2);
  assert.deepEqual(releases, [1, 2]);
});


test('snapshot upload fails closed at its deadline without an acknowledgement and releases every lease', async (t) => {
  let now = 0;
  let attempts = 0;
  let releases = 0;
  t.mock.method(Date, 'now', () => now);
  const ctx = { sync: { async leaseCollection() {
    attempts += 1;
    return {
      bridge: { state: { async pushDocumentsToRemotePeers() { now += 50; return false; } } },
      async release() { releases += 1; },
    };
  } } };
  await assert.rejects(
    confirmSnapshotDocuments(ctx, 'desktop_file_chunks', [{ id: 'unconfirmed' }], { timeoutMs: 100, retryMs: 0 }),
    (error) => error.message === 'desktop_file_chunks_snapshot_sync_timeout'
      && error.cause.message === 'desktop_file_chunks_push_unconfirmed',
  );
  assert.equal(now, 100);
  assert.equal(attempts, 2);
  assert.equal(releases, 2);
});

test('snapshot upload accepts a multi-tab leader acknowledgement', async () => {
  let released = false;
  const rows = [{ id: 'file-1' }];
  const ctx = {
    sync: {
      async leaseCollection() {
        return {
          bridge: {
            mode: 'follower',
            async flush(received) {
              assert.deepEqual(received, rows);
              return { ok: true };
            },
          },
          async release() { released = true; },
        };
      },
    },
  };
  assert.equal(await confirmSnapshotDocuments(
    ctx, 'desktop_files', rows, { timeoutMs: 100, retryMs: 0 },
  ), true);
  assert.equal(released, true);
});

test('parseGitHubUrl accepts only public GitHub repository URLs', () => {
  assert.deepEqual(parseGitHubUrl('https://github.com/AksharP5/omarchy-radio-atlas'), {
    owner: 'AksharP5',
    repo: 'omarchy-radio-atlas',
    ref: null,
    repositoryUrl: 'https://github.com/AksharP5/omarchy-radio-atlas',
  });
  assert.deepEqual(parseGitHubUrl('https://github.com/acme/app/tree/feature/radio'), {
    owner: 'acme',
    repo: 'app',
    ref: 'feature/radio',
    repositoryUrl: 'https://github.com/acme/app',
  });
  assert.equal(parseGitHubUrl('http://github.com/acme/x'), null);
  assert.equal(parseGitHubUrl('https://gitlab.com/acme/x'), null);
  assert.equal(parseGitHubUrl('https://github.com/acme/x/issues'), null);
  assert.equal(parseGitHubUrl('not a url'), null);
});

test('source filtering keeps agent-relevant runtimes and excludes generated trees', () => {
  for (const path of ['node_modules/a.js', '.git/HEAD', 'dist/a.js', 'build/a', '.next/a', 'yarn.lock']) {
    assert.equal(shouldSkipPath(path), true, path);
  }
  for (const path of ['shell.qml', 'main.py', 'install.sh', 'Makefile', 'src/radio']) {
    assert.equal(isImportableFile(path), true, path);
    assert.equal(isTextFile(path), true, path);
  }
  assert.equal(isImportableFile('assets/globe.png'), true);
  assert.equal(isTextFile('assets/globe.png'), false);
  assert.equal(isImportableFile('dist/generated.js'), false);
});

test('module ids follow the launcher slug contract', () => {
  assert.equal(moduleIdFromSource('omarchy-radio-atlas'), 'omarchy-radio-atlas');
  assert.equal(moduleIdFromSource('Radio Atlas!'), 'radio-atlas');
  assert.equal(moduleIdFromSource('black-hole-standalone-v6.html'), 'black-hole');
  assert.equal(moduleIdFromSource('my-noise-pet-crt-v5 2.html'), 'my-noise-pet-crt');
  assert.equal(validModuleId('radio-atlas'), true);
  assert.equal(validModuleId('-bad'), false);
  assert.equal(validModuleId('Bad'), false);
  assert.equal(validModuleId('x'), false);
});

test('standalone HTML imports derive clean app identity and an explicit profile', () => {
  assert.deepEqual(appIdentityFromSource(
    'blocks-ce-worldfixed-lighting-v2.html',
    '<title>Blocks CE – WebGL Castle Builder</title>',
  ), { moduleId: 'blocks-ce', appTitle: 'Blocks CE' });
  assert.deepEqual(appIdentityFromSource(
    'black-hole-standalone-v6.html',
    '<title>Black Hole Studio · Standalone</title>',
  ), { moduleId: 'black-hole-studio', appTitle: 'Black Hole Studio' });
  assert.equal(standaloneHtmlEntryPath(['black-hole-standalone-v6.html']), 'black-hole-standalone-v6.html');
  assert.equal(standaloneHtmlEntryPath(['index.html', 'icon.svg']), null);
  assert.equal(importFileByteLimit(['black-hole.html']), 8 * 1024 * 1024);
  assert.equal(importFileByteLimit(['main.qml']), 512 * 1024);
  assert.equal(importFileByteLimit(['index.html', 'icon.svg']), 512 * 1024);
  assert.equal(normalizeImportCategory('Unterhaltung'), 'entertainment');
});

test('native-peer acknowledgement timeouts keep the durable job observable', () => {
  assert.equal(isRecoverableDispatchError(
    new Error('WebRTC native peer did not open for business_commands within 25000ms; reconnect repair is scheduled.'),
  ), true);
  assert.equal(isRecoverableDispatchError(Object.assign(
    new Error('CTOX wartet noch auf die Rueckmeldung. Der Vorgang bleibt verfolgbar.'),
    {
      code: 'projection_delayed',
      transient: true,
      receipt: { command_id: 'app-import-radio-42' },
    },
  )), true);
  assert.equal(isRecoverableDispatchError(Object.assign(
    new Error('temporary pre-insert failure'),
    { transient: true },
  )), false);
  assert.equal(isRecoverableDispatchError(new Error('permission denied')), false);
  assert.equal(isRecoverableDispatchError(new Error('command_bus_unavailable')), false);
});

test('GitHub import creates one durable harness command with the porting skill', () => {
  const command = buildAppImportCommand({
    moduleId: 'omarchy-radio-atlas',
    appTitle: 'Omarchy Radio Atlas',
    category: 'entertainment',
    now: 42,
    importSource: {
      kind: 'github',
      repository_url: 'https://github.com/AksharP5/omarchy-radio-atlas',
      ref: 'main',
    },
  });
  assert.equal(command.id, 'app-import-omarchy-radio-atlas-42');
  assert.equal(command.command_id, 'app-import-omarchy-radio-atlas-42');
  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
  assert.equal(command.payload.category, 'entertainment');
  assert.equal(command.payload.desired_version, '1.0.0');
  assert.equal(command.sync_flush_timeout_ms, 15_000);
  assert.equal(command.allow_dependency_delivery_lag, false);
  assert.deepEqual(command.payload.required_skills, ['business-os-app-module-development']);
  assert.deepEqual(command.payload.import_source, {
    kind: 'github',
    repository_url: 'https://github.com/AksharP5/omarchy-radio-atlas',
    ref: 'main',
  });
  assert.equal(command.client_context.source, 'business-os-app-importer');
});

test('folder imports declare exact RxDB dependencies', () => {
  const command = buildAppImportCommand({
    moduleId: 'local-radio',
    now: 7,
    importSource: {
      kind: 'desktop-folder',
      snapshot_id: 'snapshot-1',
      files: [{
        file_id: 'file-1', generation_id: 'gen-1', relative_path: 'main.qml', sha256: 'abc', size_bytes: 12,
      }],
    },
  });
  assert.deepEqual(command.sync_collections, ['desktop_files', 'desktop_file_chunks']);
  assert.equal(command.allow_dependency_delivery_lag, true);
  assert.deepEqual(command.dependencies, [{
    collection: 'desktop_files', record_id: 'file-1', generation_id: 'gen-1', content_hash: 'abc', required: true,
  }]);
});

test('presentation is a one-click source, porting, live flow', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

  assert.match(html, /data-imp-step="source"/);
  assert.match(html, /data-imp-step="progress"/);
  assert.match(html, /data-imp-step="done"/);
  assert.match(html, /data-imp-category/);
  assert.equal((html.match(/data-imp-phase/g) || []).length, 5);
  assert.doesNotMatch(html, /data-imp-install/);
  assert.doesNotMatch(html, /data-imp-back/);
  assert.doesNotMatch(html, /data-imp-pick-files-input[^>]*\bmultiple\b/);

  assert.match(css, /\.imp-job-phases/);
  assert.match(css, /prefers-reduced-motion:\s*reduce/);

  assert.match(js, /commandBus\.dispatch\(command, \{ until: 'accepted'/);
  assert.match(js, /Job secured locally; waiting for CTOX sync/);
  assert.match(js, /showDirectoryPicker\(\{ mode: 'read' \}\)/);
  assert.doesNotMatch(js, /transcodeApp|scaffoldModule|createWritable|mode: 'readwrite'/);
  assert.match(js, /result\.live === true/);
  assert.match(js, /result\.smoke_status/);
  assert.match(js, /target\.search = source\.search/);

  assert.equal(manifest.layout.shell, 'windowed');
  assert.deepEqual(manifest.presentation.initial_size, { width: 860, height: 600 });
  assert.deepEqual(manifest.presentation.minimum_size, { width: 640, height: 480 });
});
