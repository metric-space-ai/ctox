"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

function main() {
  const platformArg = valueAfter("--platform") || process.platform;
  const releaseRoot = path.join(__dirname, "..", "release");
  if (platformArg === "mac" || platformArg === "darwin") {
    const appPath = valueAfter("--app") || findMacAppBundle(releaseRoot);
    assert.ok(fs.existsSync(appPath), `macOS app bundle is missing: ${appPath}`);
    execFileSync("codesign", ["--verify", "--deep", "--strict", "--verbose=2", appPath], { stdio: "pipe" });
    execFileSync("spctl", ["--assess", "--type", "execute", "--verbose=2", appPath], { stdio: "pipe" });
    console.log(`desktop signed artifact smoke OK (mac): ${appPath}`);
    return;
  }
  throw new Error(`signed artifact smoke is not implemented for platform: ${platformArg}`);
}

function findMacAppBundle(releaseRoot) {
  const candidates = [
    path.join(releaseRoot, `mac-${process.arch}`, "CTOX Business-OS Desktop.app"),
    path.join(releaseRoot, "mac", "CTOX Business-OS Desktop.app"),
    path.join(releaseRoot, "mac-universal", "CTOX Business-OS Desktop.app"),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }
  return findFirstAppBundle(releaseRoot) || candidates[0];
}

function findFirstAppBundle(root) {
  if (!fs.existsSync(root)) return "";
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const fullPath = path.join(root, entry.name);
    if (entry.isDirectory() && entry.name.endsWith(".app")) return fullPath;
    if (entry.isDirectory()) {
      const nested = findFirstAppBundle(fullPath);
      if (nested) return nested;
    }
  }
  return "";
}

function valueAfter(flag) {
  const index = process.argv.indexOf(flag);
  if (index < 0) return "";
  return process.argv[index + 1] || "";
}

main();
