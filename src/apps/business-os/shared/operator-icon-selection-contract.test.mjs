import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { OPERATOR_ICON_SELECTION } from './operator-icon-selection.js';
import { resolveLauncherIcon } from './launcher-icon.js';

const sharedRoot = dirname(fileURLToPath(import.meta.url));
const businessOsRoot = resolve(sharedRoot, '..');
const manifestPath = resolve(
  sharedRoot,
  'assets/workjet-icons/operator-selection-v1/manifest.json',
);
const registryPath = resolve(businessOsRoot, 'modules/registry.json');

test('all 35 operator-selected raster icons are hash-bound and registered', async () => {
  const [manifest, registry] = await Promise.all([
    readFile(manifestPath, 'utf8').then(JSON.parse),
    readFile(registryPath, 'utf8').then(JSON.parse),
  ]);

  assert.equal(manifest.schema, 'workjet.operator-icon-selection.v1');
  assert.equal(manifest.assetKind, 'raster-reference');
  assert.equal(manifest.count, 35);
  assert.equal(manifest.icons.length, 35);
  assert.equal(new Set(manifest.icons.map(({ appId }) => appId)).size, 35);
  assert.deepEqual(Object.keys(OPERATOR_ICON_SELECTION).sort(), manifest.icons.map(({ appId }) => appId).sort());

  for (const icon of manifest.icons) {
    assert.match(icon.candidateId, /^candidate-(?:0[1-9]|1[0-6])$/);
    assert.match(icon.sha256, /^[a-f0-9]{64}$/);
    assert.equal(icon.mediaType, 'image/jpeg');

    const moduleDefinition = registry.modules.find(({ id }) => id === icon.appId);
    assert.ok(moduleDefinition, `missing registry module ${icon.appId}`);
    assert.equal(moduleDefinition.layout.icon_asset, icon.renderAsset);
    assert.equal(moduleDefinition.layout.icon_asset_sha256, icon.renderSha256);
    assert.equal(moduleDefinition.layout.icon_selection_sha256, icon.sha256);
    assert.equal(moduleDefinition.layout.icon_selection_candidate, icon.candidateId);
    assert.equal(moduleDefinition.layout.icon_asset_kind, 'raster-reference');

    const shellIcon = OPERATOR_ICON_SELECTION[icon.appId];
    assert.equal(shellIcon.asset, icon.renderAsset);
    assert.equal(shellIcon.sha256, icon.renderSha256);
    assert.equal(shellIcon.candidateId, icon.candidateId);

    const originalBytes = await readFile(resolve(businessOsRoot, icon.asset));
    const actualOriginalSha256 = createHash('sha256').update(originalBytes).digest('hex');
    assert.equal(actualOriginalSha256, icon.sha256, `selection hash mismatch for ${icon.appId}`);

    const renderBytes = await readFile(resolve(businessOsRoot, icon.renderAsset));
    const actualRenderSha256 = createHash('sha256').update(renderBytes).digest('hex');
    assert.equal(actualRenderSha256, icon.renderSha256, `render hash mismatch for ${icon.appId}`);
  }
});

test('launcher renders raster-backed modules even when they intentionally have no inline svg', async () => {
  const registry = await readFile(registryPath, 'utf8').then(JSON.parse);
  const rasterOnlyAppIds = [
    'consent',
    'esign',
    'intake',
    'interviews',
    'knowledge',
    'nachweise',
    'placements',
    'submissions',
    'support',
  ];

  for (const id of rasterOnlyAppIds) {
    const moduleDefinition = registry.modules.find((moduleDef) => moduleDef.id === id);
    assert.ok(moduleDefinition, `missing registry module ${id}`);
    assert.equal(moduleDefinition.layout.icon_svg, undefined, `${id} must exercise the raster-only path`);
    assert.ok(moduleDefinition.layout.icon_asset, `${id} needs a registered raster asset`);
    assert.deepEqual(
      resolveLauncherIcon({ kind: 'module', id, module: moduleDefinition }),
      { kind: 'raster', asset: moduleDefinition.layout.icon_asset },
    );
  }
});

test('desktop-only apps fail closed instead of borrowing another product icon', async () => {
  const appSource = await readFile(resolve(businessOsRoot, 'app.js'), 'utf8');
  const desktopAppsStart = appSource.indexOf('const DESKTOP_APPS = [');
  const desktopAppsSource = appSource.slice(
    desktopAppsStart,
    appSource.indexOf('];', desktopAppsStart) + 2,
  );
  assert.doesNotMatch(desktopAppsSource, /iconAsset\s*:/, 'desktop-only apps must not borrow operator assets');
  for (const id of ['explorer', 'file-viewer', 'code-editor']) {
    assert.deepEqual(
      resolveLauncherIcon(
        { kind: 'app', id, title: id, app: {} },
        { fallbackSvg: `<svg data-app="${id}"></svg>` },
      ),
      { kind: 'svg', markup: `<svg data-app="${id}"></svg>` },
    );
  }
});
