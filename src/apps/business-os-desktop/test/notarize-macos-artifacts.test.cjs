"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const notarizeMacosArtifacts = require("../scripts/notarize-macos-artifacts.cjs");

test("macOS artifact notarization selects only DMG containers", () => {
  assert.deepEqual(
    notarizeMacosArtifacts.findDmgArtifacts(["release/app.zip", "release/app.dmg", "release/latest-mac.yml"]),
    ["release/app.dmg"],
  );
});

test("macOS artifact notarization is a no-op without a DMG", async () => {
  await notarizeMacosArtifacts({ artifactPaths: ["release/app.zip"] });
});
