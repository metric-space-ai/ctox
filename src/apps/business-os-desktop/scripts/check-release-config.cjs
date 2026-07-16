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
  assert.equal(packageJson.homepage, "https://ctox.dev");
  assert.equal(packageJson.author, "CTOX <oss@ctox.dev>");
  assert.match(packageJson.author, /<[^@\s<>]+@[^@\s<>]+\.[^@\s<>]+>/, "package author must include an email");
  assert.equal(packageJson.dependencies?.["electron-updater"], "^6.8.3");
  assert.equal(packageJson.devDependencies?.electron, "^39.8.10");
  assert.equal(packageJson.devDependencies?.["electron-builder"], "^26.8.1");
  for (const script of [
    "dist",
    "pack:dir",
    "pack:dir:bundled-runtime-smoke",
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
  assert.equal(builderConfig.productName, "CTOX Business-OS Desktop Beta");
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
  assert.equal(builderConfig.linux?.maintainer, packageJson.author);
  assert.equal(builderConfig.appx?.identityName, "MichaelWelsch.ctox");
  assert.equal(builderConfig.appx?.publisher, "CN=A8C36C19-A31B-4FA0-8621-2C0AB781EA66");
  assert.equal(builderConfig.appx?.publisherDisplayName, "Michael Welsch");
  const notarizeSource = fs.readFileSync(path.join(appRoot, builderConfig.afterSign), "utf8");
  assert.match(notarizeSource, /isDirOnlyPack/);

  const mainSource = fs.readFileSync(path.join(appRoot, packageJson.main), "utf8");
  assert.match(mainSource, /configureAutoUpdates/);
  assert.match(mainSource, /electron-updater/);

  assertLockfileIntegrity();
  assertCiWorkflowMatrix();
  assertReleaseWorkflowMatrix();
  assertDedicatedDesktopReleaseWorkflow();

  console.log("desktop release config OK");
}

function assertDedicatedDesktopReleaseWorkflow() {
  const workflowPath = path.join(repoRoot, ".github", "workflows", "business-os-desktop-release.yml");
  const workflow = fs.readFileSync(workflowPath, "utf8");
  assert.match(workflow, /business-os-desktop-v\*/);
  for (const artifact of [
    "ctox-business-os-desktop-macos-arm64",
    "ctox-business-os-desktop-macos-x64",
    "ctox-business-os-desktop-linux-x64",
    "ctox-business-os-desktop-windows-x64",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(`artifact: ${artifact}`)));
  }
  assert.match(workflow, /CTOX_WINDOWS_STORE_RELEASE/);
  assert.match(workflow, /Verify macOS signing secrets/);
  assert.match(workflow, /actions\/attest-build-provenance@0f67c3f4856b2e3261c31976d6725780e5e4c373/);
  assert.match(workflow, /softprops\/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65/);
}

function assertLockfileIntegrity() {
  const lock = JSON.parse(fs.readFileSync(path.join(appRoot, "package-lock.json"), "utf8"));
  assert.equal(lock.lockfileVersion, 3, "package-lock.json must be lockfileVersion 3");
  const missing = [];
  for (const [name, entry] of Object.entries(lock.packages || {})) {
    if (!name) continue; // the root package legitimately has no integrity
    if (entry.link) continue; // local workspace links carry no integrity
    if (!entry.integrity) missing.push(name);
  }
  // npm ci (used by CI and release) can only verify downloaded tarballs against a
  // pinned subresource-integrity hash if every package entry has one. A stripped
  // lockfile defeats the purpose of committing it for a code-signed artifact.
  assert.equal(
    missing.length,
    0,
    `package-lock.json has ${missing.length} entries without integrity (npm ci cannot verify supply chain): `
    + `${missing.slice(0, 5).join(", ")}${missing.length > 5 ? ` (+${missing.length - 5} more)` : ""}`,
  );
}

function assertCiWorkflowMatrix() {
  const ciWorkflowPath = path.join(repoRoot, ".github", "workflows", "ci.yml");
  const workflow = fs.readFileSync(ciWorkflowPath, "utf8");
  assert.match(workflow, /^\s{2}check-business-os-desktop:/m, "CI workflow is missing Business OS Desktop job");
  for (const platform of ["platform: mac", "platform: linux", "platform: win"]) {
    assert.match(workflow, new RegExp(escapeRegExp(platform)), `CI workflow missing Desktop platform: ${platform}`);
  }
  for (const command of [
    "npm ci",
    "npm test",
    "npm run check",
    "npm run release:check",
    "npm run pack:dir:bundled-runtime-smoke",
    "npm run test:electron-smoke",
    "xvfb-run -a npm run test:electron-smoke",
    "npm run smoke:keychain-runtime",
    "dbus-run-session",
    "gnome-keyring",
    "libsecret-tools",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(command)), `CI workflow missing command: ${command}`);
  }
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
    "business-os desktop invite --format json",
    "Desktop Release Helper Smoke",
    "desktop-invite.json",
    "CTOX_WINDOWS_STORE_RELEASE",
    "npm run dist -- --${{ matrix.builderPlatform }} --${{ matrix.arch }} --publish never",
    "npm run smoke:signed-artifacts -- --platform ${{ matrix.builderPlatform }} --evidence-json release/artifact-smoke-${{ matrix.builderPlatform }}-${{ matrix.arch }}.json",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(command)), `release workflow missing command: ${command}`);
  }
  assert.match(workflow, /Verify macOS signing secrets/, "release workflow must preflight macOS signing secrets");
  assert.match(
    workflow,
    /for variable in APPLE_ID APPLE_ID_PASSWORD APPLE_TEAM_ID CSC_LINK CSC_KEY_PASSWORD;/,
    "release workflow missing complete macOS secret preflight variable list",
  );
  assert.match(workflow, /::error::\$\{variable\} is required/, "release workflow must emit explicit missing-secret errors");
  const signedArtifactSmoke = fs.readFileSync(path.join(appRoot, "scripts", "smoke-signed-artifacts.cjs"), "utf8");
  for (const platform of ["smokeMacArtifacts", "smokeLinuxArtifacts", "smokeWindowsArtifacts"]) {
    assert.match(signedArtifactSmoke, new RegExp(escapeRegExp(platform)), `signed artifact smoke missing ${platform}`);
  }
  assert.match(signedArtifactSmoke, /ctox-business-os-desktop-release-artifact-smoke\/v1/);
  assert.match(signedArtifactSmoke, /writeEvidence/);
  // The Windows artifact smoke must inspect the Authenticode signature so an
  // unsigned NSIS installer is never shipped silently, and must support a hard
  // --require-signature gate for when a signing certificate is configured.
  assert.match(signedArtifactSmoke, /Get-AuthenticodeSignature/, "windows artifact smoke must check Authenticode signature");
  assert.match(signedArtifactSmoke, /MichaelWelsch\\\.ctox/, "windows Store smoke must verify the package identity");
  assert.match(signedArtifactSmoke, /--require-signature/, "windows artifact smoke must support enforcing a signature");
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
    /needs:\s*\[(?:[\w-]+,\s*)*build-desktop-macos,\s*build-desktop-linux,\s*build-desktop-windows,\s*build-business-os-desktop,\s*build-ctox\]/,
    "GitHub release job must wait for Business OS Desktop artifacts",
  );
  assert.match(workflow, /actions\/attest-build-provenance@0f67c3f4856b2e3261c31976d6725780e5e4c373/);
  assert.match(workflow, /subject-path:\s*artifacts\/\*\*\/\*/);
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

main();
