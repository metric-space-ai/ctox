"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const notarizeMacos = require("../scripts/notarize-macos.cjs");

test("macOS notarization hook skips dir-only pack smoke", async () => {
  assert.equal(notarizeMacos.isDirOnlyPack({ targets: [{ name: "dir" }] }), true);
  assert.equal(notarizeMacos.isDirectoryPackRequested(["electron-builder", "--dir"]), true);
  await notarizeMacos({
    electronPlatformName: "darwin",
    targets: [{ name: "dir" }],
  });
});

test("macOS notarization hook fails closed for distribution targets without build secrets", async () => {
  assert.equal(notarizeMacos.isDirOnlyPack({ targets: [{ name: "dmg" }] }), false);
  await assert.rejects(
    () => notarizeMacos({
      electronPlatformName: "darwin",
      targets: [{ name: "dmg" }],
      appOutDir: "/tmp/out",
      packager: {
        appInfo: {
          productFilename: "CTOX Business-OS Desktop",
        },
      },
    }),
    /requires APPLE_ID/,
  );
});
