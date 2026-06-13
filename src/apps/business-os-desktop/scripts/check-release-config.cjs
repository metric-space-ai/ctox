"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const appRoot = path.join(__dirname, "..");
const repoRoot = path.resolve(appRoot, "../../..");
const packageJson = JSON.parse(fs.readFileSync(path.join(appRoot, "package.json"), "utf8"));
const builderConfig = require(path.join(appRoot, "electron-builder.config.cjs"));

function main() {
  assert.equal(packageJson.main, "src/main/main.cjs");
  assert.equal(packageJson.author, "Metric Space AI");
  assert.equal(packageJson.dependencies?.["electron-updater"], "^6.8.3");
  assert.equal(packageJson.devDependencies?.electron, "^39.8.10");
  assert.equal(packageJson.devDependencies?.["electron-builder"], "^26.8.1");
  for (const script of [
    "dist",
    "pack:dir",
    "pack:dir:smoke",
    "release:check",
    "smoke:local-bundled-runtime",
    "smoke:local-runtime",
    "smoke:signed-artifacts",
  ]) {
    assert.ok(packageJson.scripts?.[script], `missing package script: ${script}`);
  }
  assert.equal(packageJson.devDependencies?.["@electron/asar"], "^3.4.1");

  assert.equal(builderConfig.appId, "ai.metric-space.ctox.business-os-desktop");
  assert.equal(builderConfig.productName, "CTOX Business-OS Desktop");
  assert.equal(builderConfig.asar, true);
  assert.equal(builderConfig.icon, "build/icon.png");
  assert.ok(builderConfig.files.includes("src/**/*"), "packaging must include app source");
  assert.ok(builderConfig.files.includes("!test/**"), "packaging must exclude tests");
  assert.ok(builderConfig.files.includes("!release/**"), "packaging must exclude release artifacts");
  assert.ok(builderConfig.protocols?.some((entry) => entry.schemes?.includes("ctox-business-os-desktop")));
  const builderSource = fs.readFileSync(path.join(appRoot, "electron-builder.config.cjs"), "utf8");
  assert.match(builderSource, /extraResources/, "packaging must support external helper resources");
  assert.match(builderSource, /resources["']?,\s*["']ctox/, "packaging must use resources/ctox for the helper");

  const publish = builderConfig.publish?.[0] || {};
  assert.equal(publish.provider, "generic");
  assert.match(publish.url || "", /^https:\/\//);
  assert.equal(builderConfig.generateUpdatesFilesForAllChannels, true);

  assert.equal(builderConfig.mac?.hardenedRuntime, true);
  assert.equal(builderConfig.mac?.icon, "build/icon.icns");
  assert.equal(builderConfig.mac?.entitlements, "build/entitlements.mac.plist");
  assert.equal(builderConfig.mac?.entitlementsInherit, "build/entitlements.mac.plist");
  assert.equal(builderConfig.afterSign, "scripts/notarize-macos.cjs");
  assert.ok(fs.existsSync(path.join(appRoot, builderConfig.mac.entitlements)), "macOS entitlements file is missing");
  assert.ok(fs.existsSync(path.join(appRoot, builderConfig.icon)), "desktop icon png is missing");
  assert.ok(fs.existsSync(path.join(appRoot, builderConfig.mac.icon)), "macOS icon icns is missing");
  assert.equal(builderConfig.linux?.icon, "build/icon.png");
  const notarizeSource = fs.readFileSync(path.join(appRoot, builderConfig.afterSign), "utf8");
  assert.match(notarizeSource, /isDirOnlyPack/);

  const mainSource = fs.readFileSync(path.join(appRoot, packageJson.main), "utf8");
  assert.match(mainSource, /configureAutoUpdates/);
  assert.match(mainSource, /electron-updater/);

  assertReleaseWorkflowMatrix();

  console.log("desktop release config OK");
}

function assertReleaseWorkflowMatrix() {
  const releaseWorkflowPath = path.join(repoRoot, ".github", "workflows", "release.yml");
  const workflow = fs.readFileSync(releaseWorkflowPath, "utf8");
  assert.match(workflow, /^\s{2}build-business-os-desktop:/m, "release workflow is missing Business OS Desktop job");
  for (const artifact of [
    "ctox-business-os-desktop-macos-arm64",
    "ctox-business-os-desktop-macos-x64",
    "ctox-business-os-desktop-linux-x64",
    "ctox-business-os-desktop-windows-x64",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(`artifact: ${artifact}`)), `release workflow missing ${artifact}`);
  }
  for (const command of [
    "npm ci",
    "npm test",
    "npm run check",
    "npm run release:check",
    "npm run test:electron-smoke",
    "npm run smoke:keychain-runtime",
    "dbus-run-session",
    "cargo build --locked --release",
    "resources/ctox",
    "npm run dist -- --${{ matrix.builderPlatform }} --${{ matrix.arch }} --publish never",
    "npm run smoke:signed-artifacts -- --platform ${{ matrix.builderPlatform }} --evidence-json release/artifact-smoke-${{ matrix.builderPlatform }}-${{ matrix.arch }}.json",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(command)), `release workflow missing command: ${command}`);
  }
  const signedArtifactSmoke = fs.readFileSync(path.join(appRoot, "scripts", "smoke-signed-artifacts.cjs"), "utf8");
  for (const platform of ["smokeMacArtifacts", "smokeLinuxArtifacts", "smokeWindowsArtifacts"]) {
    assert.match(signedArtifactSmoke, new RegExp(escapeRegExp(platform)), `signed artifact smoke missing ${platform}`);
  }
  assert.match(signedArtifactSmoke, /ctox-business-os-desktop-release-artifact-smoke\/v1/);
  assert.match(signedArtifactSmoke, /writeEvidence/);
  for (const linuxDependency of ["gnome-keyring", "libsecret-tools"]) {
    assert.match(workflow, new RegExp(escapeRegExp(linuxDependency)), `release workflow missing Linux keychain dependency: ${linuxDependency}`);
  }
  for (const secret of [
    "secrets.APPLE_ID",
    "secrets.APPLE_ID_PASSWORD",
    "secrets.APPLE_TEAM_ID",
    "secrets.CTOX_BUSINESS_OS_DESKTOP_CSC_LINK",
    "secrets.CTOX_BUSINESS_OS_DESKTOP_CSC_KEY_PASSWORD",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(secret)), `release workflow missing secret: ${secret}`);
  }
  assert.match(
    workflow,
    /needs:\s*\[build-desktop-macos,\s*build-desktop-linux,\s*build-desktop-windows,\s*build-business-os-desktop,\s*build-ctox\]/,
    "GitHub release job must wait for Business OS Desktop artifacts",
  );
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

main();
