import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const boxArtUrl = new URL("../box-art.mjs", import.meta.url);
const storeShelfUrl = new URL("../store-shelf.mjs", import.meta.url);
const provenanceUrl = new URL("../provenance.json", import.meta.url);

const [boxArtSource, storeShelfSource, provenanceSource] = await Promise.all([
  readFile(boxArtUrl, "utf8"),
  readFile(storeShelfUrl, "utf8"),
  readFile(provenanceUrl, "utf8"),
]);

test("browser modules parse in Node without DOM or WebGL globals", async () => {
  const [boxArt, storeShelf] = await Promise.all([
    import(boxArtUrl.href),
    import(storeShelfUrl.href),
  ]);

  assert.equal(typeof boxArt.createAppPackageTexture, "function");
  assert.equal(typeof boxArt.resolvePackagePalette, "function");
  assert.equal(typeof boxArt.palettes, "object");
  assert.equal(typeof storeShelf.createStoreShelf, "function");
});

test("store shelf source pins the public API and source animation model", () => {
  assert.match(storeShelfSource, /export function createStoreShelf\s*\(/);
  for (const method of ["select", "deselect", "setApps", "currentIndex", "destroy"]) {
    assert.match(storeShelfSource, new RegExp(`\\b${method}\\b`));
  }
  assert.match(storeShelfSource, /function damp\s*\(/);
  assert.match(storeShelfSource, /groupCount \* 0\.56/);
  assert.match(storeShelfSource, /stage\.clientWidth < 900/);
  assert.match(storeShelfSource, /stage\.clientWidth < 600/);
  assert.match(storeShelfSource, /Math\.min\(globalThis\.devicePixelRatio \|\| 1, 1\.75\)/);
  assert.match(storeShelfSource, /new ResizeObserver\(resize\)/);
});

test("scroll uses the supplied container and exact track-length denominator", () => {
  assert.match(
    storeShelfSource,
    /track\.offsetHeight - scrollContainer\.clientHeight/,
  );
  assert.match(storeShelfSource, /scrollContainer\.scrollTop \/ available/);
  assert.match(
    storeShelfSource,
    /scrollContainer\.addEventListener\("scroll", onScroll, \{ passive: true \}\)/,
  );
  assert.doesNotMatch(
    storeShelfSource,
    /window\.addEventListener\(\s*["']scroll["']/,
  );
});

test("library source has only relative three imports and no framework/network hooks", () => {
  const combined = `${boxArtSource}\n${storeShelfSource}`;
  assert.doesNotMatch(combined, /from\s+["']three["']/);
  assert.doesNotMatch(combined, /\bfetch\s*\(/);
  assert.doesNotMatch(combined, /\bReact\b|\buseEffect\b|\buseLayoutEffect\b|\buseRef\b|\buseState\b/);
  assert.doesNotMatch(storeShelfSource, /createElement\s*\(|appendChild\s*\(|\.style\s*[.=]/);
  assert.match(storeShelfSource, /from "\.\.\/three\/three\.module\.min\.js"/);
  assert.match(storeShelfSource, /from "\.\.\/three\/RoundedBoxGeometry\.js"/);
});

test("box art retains palettes, localized ownership copy, motifs, and async updates", () => {
  assert.match(boxArtSource, /export const palettes\s*=/);
  assert.match(boxArtSource, /stableHash\s*\(/);
  assert.match(boxArtSource, /INSTALL · VERSION · OWN/);
  assert.match(boxArtSource, /INSTALLIEREN · VERSIONIEREN · BESITZEN/);
  assert.match(boxArtSource, /function packageMonogram\s*\(/);
  assert.match(boxArtSource, /function wrapText\s*\(/);
  assert.match(boxArtSource, /function drawCoverImage\s*\(/);
  assert.match(boxArtSource, /function drawContainedImage\s*\(/);
  assert.match(boxArtSource, /function drawPlatformBar\s*\(/);
  assert.match(boxArtSource, /function drawPaperGrain\s*\(/);
  assert.match(boxArtSource, /function drawAppMotif\s*\(/);
  assert.match(boxArtSource, /function drawFront\s*\(/);
  assert.match(boxArtSource, /function drawSpine\s*\(/);
  assert.match(boxArtSource, /function drawNoScreenshotBack\s*\(/);
  assert.match(boxArtSource, /image\.onload\s*=/);
  assert.match(boxArtSource, /texture\.needsUpdate\s*=\s*true/);
});

test("full disposal path releases shelf and renderer resources", () => {
  assert.match(storeShelfSource, /geometry\.dispose\(\)/);
  assert.match(storeShelfSource, /material\.map\.dispose\(\)/);
  assert.match(storeShelfSource, /material\.dispose\(\)/);
  assert.match(storeShelfSource, /renderer\.dispose\(\)/);
  assert.match(storeShelfSource, /cancelAnimationFrame\(frame\)/);
  assert.match(storeShelfSource, /resizeObserver\.disconnect\(\)/);
  assert.match(storeShelfSource, /removeEventListener\("scroll", onScroll\)/);
});

test("provenance records project ownership, license, date, and requested modifications", () => {
  const provenance = JSON.parse(provenanceSource);
  assert.equal(provenance.license, "AGPL-3.0-only");
  assert.equal(provenance.date, "2026-07-21");
  assert.match(provenance.provenance, /adapted from the ctox-dev marketing showcase \(same project ownership\)/);
  assert.deepEqual(provenance.sourceFiles, [
    "ctox-dev/components/marketing/app-store-showcase.tsx",
    "ctox-dev/components/marketing/app-package-template.ts",
  ]);
  assert.match(provenance.modifications, /React removed/);
  assert.match(provenance.modifications, /scroll container parameterized/);
  assert.match(provenance.modifications, /palette fallback added/);
});
