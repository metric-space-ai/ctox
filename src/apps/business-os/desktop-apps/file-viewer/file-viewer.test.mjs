import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./app.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __fileViewerTestHooks: viewer } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

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
              async awaitInSync() {
                calls.push(`in-sync:${name}`);
              },
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
