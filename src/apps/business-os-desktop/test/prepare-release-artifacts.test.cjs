"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const yaml = require("js-yaml");

const { prepareReleaseArtifacts } = require("../scripts/prepare-release-artifacts.cjs");

function writeManifest(root, artifact, arch) {
  const directory = path.join(root, artifact);
  fs.mkdirSync(directory, { recursive: true });
  const prefix = `CTOX.Business-OS.Desktop.Beta-0.3.51-mac-${arch}`;
  fs.writeFileSync(
    path.join(directory, "latest-mac.yml"),
    yaml.dump({
      version: "0.3.51",
      files: [
        { url: `${prefix}.zip`, sha512: `zip-${arch}`, size: 100 },
        { url: `${prefix}.dmg`, sha512: `dmg-${arch}`, size: 90 },
        { url: `${prefix}.dmg`, sha512: `dmg-${arch}`, size: 90 },
      ],
      path: `${prefix}.zip`,
      sha512: `zip-${arch}`,
      releaseDate: arch === "arm64" ? "2026-07-18T10:00:00Z" : "2026-07-18T10:01:00Z",
    }),
  );
}

test("release preparation merges macOS updater manifests for both architectures", () => {
  const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-release-artifacts-"));
  const inputRoot = path.join(temporaryRoot, "input");
  const outputRoot = path.join(temporaryRoot, "output");
  writeManifest(inputRoot, "ctox-business-os-desktop-macos-arm64", "arm64");
  writeManifest(inputRoot, "ctox-business-os-desktop-macos-x64", "x64");

  prepareReleaseArtifacts(inputRoot, outputRoot);

  const merged = yaml.load(fs.readFileSync(path.join(outputRoot, "latest-mac.yml"), "utf8"));
  assert.equal(merged.version, "0.3.51");
  assert.equal(merged.path, "CTOX.Business-OS.Desktop.Beta-0.3.51-mac-x64.zip");
  assert.deepEqual(
    merged.files.filter((file) => file.url.endsWith(".zip")).map((file) => file.url).sort(),
    [
      "CTOX.Business-OS.Desktop.Beta-0.3.51-mac-arm64.zip",
      "CTOX.Business-OS.Desktop.Beta-0.3.51-mac-x64.zip",
    ],
  );
  assert.equal(new Set(merged.files.map((file) => file.url)).size, merged.files.length);
  assert.doesNotMatch(fs.readFileSync(path.join(outputRoot, "latest-mac.yml"), "utf8"), /CTOX Business/);
});

test("release preparation fails closed on unexpected conflicting artifacts", () => {
  const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-release-collision-"));
  const inputRoot = path.join(temporaryRoot, "input");
  const outputRoot = path.join(temporaryRoot, "output");
  for (const [artifact, contents] of [["one", "first"], ["two", "second"]]) {
    const directory = path.join(inputRoot, artifact);
    fs.mkdirSync(directory, { recursive: true });
    fs.writeFileSync(path.join(directory, "latest-linux.yml"), contents);
  }
  assert.throws(
    () => prepareReleaseArtifacts(inputRoot, outputRoot),
    /conflicting files named latest-linux\.yml/,
  );
});
