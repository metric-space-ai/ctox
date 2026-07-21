"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const desktopRoot = path.resolve(__dirname, "..");
const repositoryRoot = path.resolve(desktopRoot, "../../..");

function source(relativePath) {
  return fs.readFileSync(path.join(repositoryRoot, relativePath), "utf8");
}

test("managed capability retry requests a fresh ctox.dev launch contract", () => {
  const preload = source("src/apps/business-os-desktop/src/instance-preload.cjs");
  const main = source("src/apps/business-os-desktop/src/main/main.cjs");
  const shell = source("src/apps/business-os/app.js");

  assert.match(preload, /refreshManagedLaunch:\s*\(\)\s*=>\s*ipcRenderer\.send\("instance:refresh-managed-launch"\)/);
  assert.match(main, /ipcMain\.on\("instance:refresh-managed-launch"/);
  assert.match(main, /instance\.source\s*!==\s*"ctox_dev"/);
  assert.match(main, /destroyInstanceView\(instanceId\);[\s\S]*await activateInstance\(instance\);/);
  assert.match(shell, /CTOX_MANAGED_CAPABILITY_MISSING/);
  assert.match(shell, /window\.ctoxBusinessOsDesktop\.refreshManagedLaunch\(\)/);
});

test("desktop version is visible in both native and renderer chrome", () => {
  const preload = source("src/apps/business-os-desktop/src/preload.cjs");
  const main = source("src/apps/business-os-desktop/src/main/main.cjs");
  const html = source("src/apps/business-os-desktop/src/renderer/index.html");
  const renderer = source("src/apps/business-os-desktop/src/renderer/app.js");

  assert.match(main, /title:\s*`CTOX Business OS Desktop Beta v\$\{app\.getVersion\(\)\}`/);
  assert.match(main, /ipcMain\.handle\("app:info"/);
  assert.match(preload, /getAppInfo:\s*\(\)\s*=>\s*ipcRenderer\.invoke\("app:info"\)/);
  assert.match(html, /id="app-version"/);
  assert.match(renderer, /appVersion\.textContent\s*=\s*version\s*\?\s*`v\$\{version\}`/);
});

test("desktop shell forces reduced motion before Chromium starts", () => {
  const main = source("src/apps/business-os-desktop/src/main/main.cjs");
  const switchOffset = main.indexOf('app.commandLine.appendSwitch("force-prefers-reduced-motion")');
  const readyOffset = main.indexOf("app.whenReady()");

  assert.notEqual(switchOffset, -1);
  assert.notEqual(readyOffset, -1);
  assert.ok(switchOffset < readyOffset, "reduced-motion switch must be set before app readiness");
});
