"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const asar = require("@electron/asar");

function main() {
  if (process.platform === "darwin") {
    smokeMacBundle();
    return;
  }
  throw new Error(`pack directory smoke is not implemented for platform: ${process.platform}`);
}

function smokeMacBundle() {
  const appRoot = path.join(__dirname, "..");
  const appPath = path.join(appRoot, "release", `mac-${process.arch}`, "CTOX Business-OS Desktop Beta.app");
  const plistPath = path.join(appPath, "Contents", "Info.plist");
  const iconPath = path.join(appPath, "Contents", "Resources", "icon.icns");
  const asarPath = path.join(appPath, "Contents", "Resources", "app.asar");
  assert.ok(fs.existsSync(appPath), `packaged app is missing: ${appPath}`);
  assert.ok(fs.existsSync(plistPath), "Info.plist is missing from packaged app");
  assert.ok(fs.existsSync(iconPath), "packaged app icon is missing");
  assert.ok(fs.existsSync(asarPath), "app.asar is missing from packaged app");

  const plist = fs.readFileSync(plistPath, "utf8");
  assert.match(plist, /ai\.metric-space\.ctox\.business-os-desktop/);
  assert.match(plist, /ctox-business-os-desktop/);

  const files = asar.listPackage(asarPath);
  assert.ok(files.includes("/src/main/main.cjs"), "main process source missing from app.asar");
  assert.ok(files.includes("/src/preload.cjs"), "preload source missing from app.asar");
  assert.ok(files.includes("/src/renderer/index.html"), "renderer source missing from app.asar");
  assert.equal(files.some((file) => file.startsWith("/test/")), false, "tests must not be packaged");
  assert.equal(files.some((file) => file.startsWith("/release/")), false, "release artifacts must not be packaged");

  console.log(`desktop pack dir smoke OK (mac): ${appPath}`);
}

main();
