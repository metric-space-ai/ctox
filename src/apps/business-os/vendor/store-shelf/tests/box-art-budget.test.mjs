import assert from 'node:assert/strict';
import test, { after } from 'node:test';
import { createAppPackageTexture } from '../box-art.mjs';

const previousCanvas = globalThis.OffscreenCanvas;
const previousImage = globalThis.Image;
const images = [];
class FixtureImage {
  constructor() { images.push(this); }
}
globalThis.Image = FixtureImage;
globalThis.OffscreenCanvas = class {
  constructor(width, height) { this.width = width; this.height = height; }
  getContext() {
    return new Proxy({}, {
      get(_, key) {
        if (key === 'measureText') return text => ({ width: String(text).length * 7 });
        if (key === 'createLinearGradient' || key === 'createRadialGradient')
          return () => ({ addColorStop() {} });
        return () => {};
      },
      set() { return true; },
    });
  }
};
after(() => {
  if (previousCanvas === undefined) delete globalThis.OffscreenCanvas;
  else globalThis.OffscreenCanvas = previousCanvas;
  if (previousImage === undefined) delete globalThis.Image;
  else globalThis.Image = previousImage;
});
const template = { id:'calendar', title:'Calendar', accent:'#ef8a67', background:'#311a14' };

test('48 shelf covers and spines fit an 80MiB raw artwork budget', () => {
  const front = createAppPackageTexture(template, 'front', { scale: 0.5 });
  const spine = createAppPackageTexture(template, 'spine', { scale: 0.5 });
  const bytes = texture => texture.image.width * texture.image.height * 4;
  assert.ok(bytes(front) <= 2 * 1024 ** 2);
  assert.ok(bytes(spine) <= 2 * 1024 ** 2);
  assert.ok((bytes(front) + bytes(spine)) * 48 <= 80 * 1024 ** 2);
  front.dispose();
  spine.dispose();
});

test('detail artwork retains the original cover resolution', () => {
  const detail = createAppPackageTexture(template, 'front');
  assert.deepEqual([detail.image.width, detail.image.height], [1536, 2160]);
  detail.dispose();
});

test('retiring artwork prevents a late image callback from uploading again', () => {
  const texture = createAppPackageTexture({ ...template, heroArtwork:'/owned-static-art.svg' }, 'front', { scale: 0.5 });
  const image = images.at(-1);
  const lateLoad = image.onload;
  const version = texture.version;
  texture.dispose();
  assert.equal(image.onload, null);
  assert.equal(image.onerror, null);
  lateLoad();
  assert.equal(texture.version, version);
});

test('invalid raster scales are rejected before canvas allocation', () => {
  for (const scale of [0, 2, NaN, Infinity])
    assert.throws(() => createAppPackageTexture(template, 'front', { scale }), RangeError);
});
