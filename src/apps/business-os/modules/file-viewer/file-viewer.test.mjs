import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { describe, it } from 'node:test';

const viewerUrl = new URL('./index.js', import.meta.url);
const viewerSource = await readFile(viewerUrl, 'utf8');
const viewerCss = await readFile(new URL('./index.css', import.meta.url), 'utf8');
const viewerHtml = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const manifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
const collectionSchemas = JSON.parse(await readFile(new URL('./collections.schema.json', import.meta.url), 'utf8'));
const bundledSource = viewerSource;
const { __fileViewerTestHooks: viewer } = await import(viewerUrl);

describe('File Viewer module contract', () => {
  it('uses the Business OS module entry contract and a fragment-only view', () => {
    // App-API 4.0: mount(root, api, {restored}) → {unmount, snapshot}; Titel über die Shell.
    assert.match(viewerSource, /export async function mount\(root, api, \{ restored \} = \{\}\)/);
    assert.match(viewerSource, /api\.shell\.setTitle\(/);
    assert.match(viewerSource, /api\.files\.get\(/);
    assert.doesNotMatch(viewerSource, /ctx\.|desktop_file_chunks|demandFileLoader|\?v=/);
    assert.doesNotMatch(viewerSource, /export const manifest/);
    assert.doesNotMatch(viewerSource, /desktop-apps\//);
    // The root stays a `ctox-workspace` main element carrying the module class;
    // layout modifiers such as `ctox-workspace--single` may sit between them.
    assert.match(viewerHtml, /^<main class="ctox-workspace(?:\s+ctox-workspace--[a-z-]+)*\s+file-viewer"/);
    assert.doesNotMatch(viewerHtml, /<!doctype|<(?:html|head|script|style)\b/i);
  });

  it('declares the required immutable core module metadata and collections', () => {
    assert.equal(manifest.id, 'file-viewer');
    assert.equal(manifest.title, 'Datei-Viewer');
    assert.equal(manifest.install_scope, 'core');
    assert.equal(manifest.default_installed, true);
    assert.equal(manifest.source, 'core');
    assert.equal(manifest.core, true);
    assert.equal(manifest.deletable, false);
    assert.equal(manifest.launch_kind, 'desktop-app');
    assert.equal(manifest.layout.shell_contract, 'v2');
    assert.equal(manifest.layout.shell_geometry_contract, 'business-os-v2-global-1');
    assert.equal(manifest.store.source_path, 'modules/file-viewer');
    assert.deepEqual(manifest.collections, ['business_commands', 'desktop_files']);
    assert.deepEqual(Object.keys(collectionSchemas.collections), manifest.collections);
  });

  it('uses only shell-sanctioned container breakpoints', () => {
    assert.match(viewerCss, /@container business-app-window \(max-width: 1024px\)/);
    assert.match(viewerCss, /@container business-app-window \(max-width: 768px\)/);
    assert.doesNotMatch(viewerCss, /@media\s*\([^)]*(?:max|min)-width/);
  });
});

describe('File Viewer helpers', () => {
  it('uses a bounded range for large text previews', () => {
    assert.deepEqual(viewer.textPreviewRangeFor('text/plain', 512 * 1024), {
      offset: 0,
      length: 256 * 1024,
    });
    assert.equal(viewer.textPreviewRangeFor('application/pdf', 512 * 1024), null);
    assert.equal(viewer.textPreviewRangeFor('text/plain', 128), null);
  });

  it('limits large text previews to the first 256 KiB of the file blob', async () => {
    const blob = new Blob(['a'.repeat(300 * 1024)], { type: 'text/plain' });
    const preview = viewer.previewBlobFor(blob, 'text/plain', blob.size);
    assert.equal(preview.size, 256 * 1024);
    assert.equal(viewer.previewBlobFor(blob, 'application/pdf', blob.size), blob);
  });

  it('normalizes route parameters from explorer, desktop and older callers', () => {
    assert.deepEqual(viewer.normalizeTarget({ fileId: 'f1', mimeType: 'image/png', sizeBytes: 3, contentState: 'lazy', path: '/CTOX/a.png' }), {
      ref: '', fileId: 'f1', name: '', mime: 'image/png', size: 3, path: '/CTOX/a.png', contentState: 'lazy',
    });
    assert.equal(viewer.normalizeTarget({ ref: 'abc' }).mime, 'application/octet-stream');
  });
});
