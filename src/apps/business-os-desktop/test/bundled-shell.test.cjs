"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { businessOsShellRoot, startBundledBusinessOsShell } = require("../src/main/bundled-shell.cjs");

test("bundled shell serves the Business OS entry and blocks traversal", async () => {
  const shell = await startBundledBusinessOsShell({
    root: path.resolve(__dirname, "../../business-os"),
  });
  try {
    const entry = await fetch(shell.url);
    assert.equal(entry.status, 200);
    assert.match(entry.headers.get("content-type"), /text\/html/);
    assert.match(await entry.text(), /CTOX Business OS/);
    const traversal = await fetch(`${shell.url}%2e%2e/business-os-desktop/package.json`);
    assert.notEqual(traversal.status, 200);
  } finally {
    await shell.close();
  }
});

test("bundled shell root resolves dev and packaged layouts", () => {
  assert.equal(
    businessOsShellRoot({ isPackaged: true, resourcesPath: "/Applications/Test.app/Contents/Resources" }),
    "/Applications/Test.app/Contents/Resources/business-os",
  );
  assert.equal(
    businessOsShellRoot({ appDir: path.resolve(__dirname, "../src/main") }),
    path.resolve(__dirname, "../../business-os"),
  );
});
