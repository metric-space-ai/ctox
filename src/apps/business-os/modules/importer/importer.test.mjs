import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import {
  buildAppImportCommand,
  isImportableFile,
  isRecoverableDispatchError,
  isTextFile,
  moduleIdFromSource,
  parseGitHubUrl,
  shouldSkipPath,
  validModuleId,
} from './index.js';

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
  assert.equal(validModuleId('radio-atlas'), true);
  assert.equal(validModuleId('-bad'), false);
  assert.equal(validModuleId('Bad'), false);
  assert.equal(validModuleId('x'), false);
});

test('native-peer acknowledgement timeouts keep the durable job observable', () => {
  assert.equal(isRecoverableDispatchError(
    new Error('WebRTC native peer did not open for business_commands within 25000ms; reconnect repair is scheduled.'),
  ), true);
  assert.equal(isRecoverableDispatchError(new Error('permission denied')), false);
  assert.equal(isRecoverableDispatchError(new Error('command_bus_unavailable')), false);
});

test('GitHub import creates one durable harness command with the porting skill', () => {
  const command = buildAppImportCommand({
    moduleId: 'omarchy-radio-atlas',
    appTitle: 'Omarchy Radio Atlas',
    now: 42,
    importSource: {
      kind: 'github',
      repository_url: 'https://github.com/AksharP5/omarchy-radio-atlas',
      ref: 'main',
    },
  });
  assert.equal(command.command_id, 'app-import-omarchy-radio-atlas-42');
  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.equal(command.payload.install_target, 'runtime-installed-module');
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
  assert.equal((html.match(/data-imp-phase/g) || []).length, 5);
  assert.doesNotMatch(html, /data-imp-install/);
  assert.doesNotMatch(html, /data-imp-back/);

  assert.match(css, /\.imp-job-phases/);
  assert.match(css, /prefers-reduced-motion:\s*reduce/);

  assert.match(js, /commandBus\.dispatch\(command, \{ until: 'accepted'/);
  assert.match(js, /Job secured locally; waiting for CTOX sync/);
  assert.match(js, /showDirectoryPicker\(\{ mode: 'read' \}\)/);
  assert.doesNotMatch(js, /transcodeApp|scaffoldModule|createWritable|mode: 'readwrite'/);
  assert.match(js, /result\.live === true/);
  assert.match(js, /result\.smoke_status/);

  assert.equal(manifest.layout.shell, 'windowed');
  assert.deepEqual(manifest.presentation.initial_size, { width: 860, height: 600 });
  assert.deepEqual(manifest.presentation.minimum_size, { width: 640, height: 480 });
});
