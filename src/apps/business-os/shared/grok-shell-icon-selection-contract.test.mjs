import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { GROK_SHELL_ICON_SELECTION, grokShellIconFor } from './grok-shell-icon-selection.js';
import { resolveLauncherIcon } from './launcher-icon.js';

const sharedRoot = dirname(fileURLToPath(import.meta.url));
const businessOsRoot = resolve(sharedRoot, '..');
const manifestPath = resolve(sharedRoot, 'assets/workjet-icons/grok-shell-v1/manifest.json');
const registryPath = resolve(businessOsRoot, 'modules/registry.json');
const appPath = resolve(businessOsRoot, 'app.js');
const pngSignature = Buffer.from('89504e470d0a1a0a', 'hex');
const digest = (value) => createHash('sha256').update(value).digest('hex');

function offlineFallbackCatalog(source) {
  const marker = 'const OFFLINE_FALLBACK_CATALOG = ';
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, 'offline fallback catalog is missing');
  const end = source.indexOf('\n};', start);
  assert.notEqual(end, -1, 'offline fallback catalog is incomplete');
  return JSON.parse(source.slice(start + marker.length, end + 2));
}

test('four Grok Agent PNG icons are hash-bound in every shell catalog', async () => {
  const [manifest, registry, appSource] = await Promise.all([
    readFile(manifestPath, 'utf8').then(JSON.parse),
    readFile(registryPath, 'utf8').then(JSON.parse),
    readFile(appPath, 'utf8'),
  ]);
  const offline = offlineFallbackCatalog(appSource);

  assert.equal(manifest.schema, 'workjet.grok-shell-icon-selection.v1');
  assert.equal(manifest.provider, 'Grok Imagine');
  assert.equal(manifest.mode, 'Agent');
  assert.equal(manifest.count, 4);
  assert.equal(manifest.icons.length, 4);
  assert.deepEqual(Object.keys(GROK_SHELL_ICON_SELECTION).sort(), manifest.icons.map((icon) => icon.appId).sort());

  for (const icon of manifest.icons) {
    assert.equal(digest(icon.prompt), icon.promptSha256, `prompt hash mismatch: ${icon.appId}`);
    assert.match(icon.sourceJpegSha256, /^[a-f0-9]{64}$/);
    assert.match(icon.reviewed60Sha256, /^[a-f0-9]{64}$/);
    assert.match(icon.postUrl, /^https:\/\/grok\.com\/imagine\/post\/[a-f0-9-]+$/);

    const selected = grokShellIconFor(icon.appId);
    assert.equal(selected?.asset, icon.renderAsset);
    assert.equal(selected?.sha256, icon.renderSha256);
    assert.equal(grokShellIconFor(`module:${icon.appId}`), selected);

    for (const catalog of [registry, offline]) {
      const moduleDef = catalog.modules.find(({ id }) => id === icon.appId);
      assert.ok(moduleDef, `missing ${icon.appId} in catalog`);
      assert.equal(moduleDef.layout.icon_asset, icon.renderAsset);
    }
    const moduleManifest = await readFile(resolve(businessOsRoot, `modules/${icon.appId}/module.json`), 'utf8').then(JSON.parse);
    assert.equal(moduleManifest.layout.icon_asset, icon.renderAsset);

    for (const [asset, expectedHash, dimension] of [
      [icon.sourceAsset, icon.sourceSha256, 1408],
      [icon.renderAsset, icon.renderSha256, 512],
    ]) {
      const bytes = await readFile(resolve(businessOsRoot, asset));
      assert.ok(bytes.subarray(0, 8).equals(pngSignature), `${asset} is not PNG`);
      assert.equal(bytes.readUInt32BE(16), dimension, `${asset} width`);
      assert.equal(bytes.readUInt32BE(20), dimension, `${asset} height`);
      assert.equal(digest(bytes), expectedHash, `${asset} hash`);
    }

    const resolved = resolveLauncherIcon({
      kind: 'module', id: icon.appId, module: { layout: { icon_svg: '<svg></svg>' } },
    });
    assert.deepEqual(resolved, { kind: 'raster', asset: icon.renderAsset });
  }

  for (const moduleDef of registry.modules) {
    assert.match(moduleDef.layout.icon_asset, /\.(?:jpe?g|png)$/i, `registered module ${moduleDef.id} has no raster icon`);
  }
});
