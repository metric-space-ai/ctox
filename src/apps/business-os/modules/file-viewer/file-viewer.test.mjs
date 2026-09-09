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
    assert.match(viewerSource, /export async function mount\(ctx\)/);
    assert.match(viewerSource, /const container = ctx\.host/);
    assert.match(viewerSource, /ctx\.setTitle\?\.\(name\)/);
    assert.doesNotMatch(viewerSource, /export const manifest/);
    assert.doesNotMatch(viewerSource, /desktop-apps\//);
    assert.match(viewerHtml, /^<main class="ctox-workspace ctox-workspace--single file-viewer"/);
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

  it('passes file ranges to the demand loader without full-content hash validation', async () => {
    const calls = [];
    const ctx = {
      sync: {
        async startCollection(name) {
          return {
            state: {
              async awaitInSync() { calls.push(`in-sync:${name}`); },
              demandFileLoader: {
                async fetchFile(fileId, options) {
                  calls.push({ fileId, options });
                  return [{ sequence: 0, bytesBase64: btoa('hello') }];
                },
              },
            },
          };
        },
      },
    };

    const blob = await viewer.readStoredFile(ctx, 'file-1', 'text/plain', {
      contentHash: 'not-the-partial-hash',
      contentHashScheme: 'sha256-bytes-v1',
      range: { offset: 0, length: 5 },
    });

    assert.equal(await blob.text(), 'hello');
    assert.deepEqual(calls.find((call) => typeof call === 'object'), {
      fileId: 'file-1',
      options: { range: { offset: 0, length: 5 } },
    });
  });

  it('keeps full file reads available when no range is requested', async () => {
    const calls = [];
    const ctx = {
      sync: {
        async startCollection() {
          return {
            state: {
              async awaitInSync() {},
              demandFileLoader: {
                async fetchFile(fileId, options) {
                  calls.push({ fileId, options });
                  return [{ sequence: 0, bytesBase64: btoa('full') }];
                },
              },
            },
          };
        },
      },
    };

    const blob = await viewer.readStoredFile(ctx, 'file-2', 'text/plain');

    assert.equal(await blob.text(), 'full');
    assert.deepEqual(calls[0], { fileId: 'file-2', options: undefined });
  });
});
