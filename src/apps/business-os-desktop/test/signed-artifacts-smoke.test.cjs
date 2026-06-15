"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const test = require("node:test");

const scriptPath = path.join(__dirname, "..", "scripts", "smoke-signed-artifacts.cjs");

test("signed artifact smoke writes platform evidence for all release targets", () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-artifacts-"));
  try {
    createMacFixture(tmp);
    createLinuxFixture(tmp);
    createWindowsFixture(tmp);

    const macEvidence = runSmoke(tmp, "mac", ["--skip-signature"]);
    const linuxEvidence = runSmoke(tmp, "linux");
    const winEvidence = runSmoke(tmp, "win");

    assertEvidence(macEvidence, "mac", ["appBundle", "infoPlist", "appAsar", "bundledHelper", "signature"]);
    assert.equal(macEvidence.checks.signature.skipped, true);
    assert.equal(macEvidence.checks.bundledHelper.executable, true);
    assertEvidence(linuxEvidence, "linux", ["appImage", "deb", "unpackedApp", "appAsar", "bundledHelper"]);
    assert.equal(linuxEvidence.checks.bundledHelper.executable, true);
    assertEvidence(winEvidence, "win", ["nsisInstaller", "unpackedApp", "appAsar", "bundledHelper"]);
    assert.match(winEvidence.checks.bundledHelper.path, /ctox\.exe$/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

function runSmoke(releaseRoot, platform, extraArgs = []) {
  const evidencePath = path.join(releaseRoot, `evidence-${platform}.json`);
  execFileSync(process.execPath, [
    scriptPath,
    "--platform",
    platform,
    "--release-root",
    releaseRoot,
    "--evidence-json",
    evidencePath,
    ...extraArgs,
  ]);
  return JSON.parse(fs.readFileSync(evidencePath, "utf8"));
}

function assertEvidence(evidence, platform, checkKeys) {
  assert.equal(evidence.schema, "ctox-business-os-desktop-release-artifact-smoke/v1");
  assert.equal(evidence.ok, true);
  assert.equal(evidence.platform, platform);
  assert.equal(evidence.releaseRoot, ".");
  assert.match(evidence.generatedAt, /^\d{4}-\d{2}-\d{2}T/);
  for (const key of checkKeys) {
    assert.ok(evidence.checks[key], `missing evidence check: ${key}`);
    if (evidence.checks[key].path) {
      assert.equal(path.isAbsolute(evidence.checks[key].path), false, `${key} path should be relative`);
    }
    if (evidence.checks[key].sizeBytes !== undefined) {
      assert.ok(evidence.checks[key].sizeBytes > 0, `${key} should record a nonempty artifact`);
    }
  }
}

function createMacFixture(root) {
  const app = path.join(root, "mac-arm64", "CTOX Business-OS Desktop Beta.app");
  writeFile(path.join(app, "Contents", "Info.plist"), "<plist></plist>");
  writeFile(path.join(app, "Contents", "Resources", "app.asar"), "asar");
  writeExecutable(path.join(app, "Contents", "Resources", "ctox", "ctox"), "#!/bin/sh\n");
}

function createLinuxFixture(root) {
  writeFile(path.join(root, "CTOX Business-OS Desktop Beta.AppImage"), "appimage");
  writeFile(path.join(root, "ctox-business-os-desktop.deb"), "deb");
  writeFile(path.join(root, "linux-unpacked", "resources", "app.asar"), "asar");
  writeExecutable(path.join(root, "linux-unpacked", "resources", "ctox", "ctox"), "#!/bin/sh\n");
}

function createWindowsFixture(root) {
  writeFile(path.join(root, "CTOX Business-OS Desktop Beta Setup.exe"), "installer");
  writeFile(path.join(root, "win-unpacked", "resources", "app.asar"), "asar");
  writeFile(path.join(root, "win-unpacked", "resources", "ctox", "ctox.exe"), "binary");
}

function writeFile(filePath, content) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content);
}

function writeExecutable(filePath, content) {
  writeFile(filePath, content);
  fs.chmodSync(filePath, 0o755);
}
