"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

function main() {
  const platform = normalizePlatform(valueAfter("--platform") || process.platform);
  const releaseRoot = path.resolve(valueAfter("--release-root") || path.join(__dirname, "..", "release"));
  const skipSignature = hasFlag("--skip-signature");
  const evidenceJson = valueAfter("--evidence-json");
  if (platform === "mac") {
    const evidence = smokeMacArtifacts(releaseRoot, { skipSignature });
    writeEvidence(evidenceJson, evidence);
    return;
  }
  if (platform === "linux") {
    const evidence = smokeLinuxArtifacts(releaseRoot);
    writeEvidence(evidenceJson, evidence);
    return;
  }
  if (platform === "win") {
    const evidence = smokeWindowsArtifacts(releaseRoot);
    writeEvidence(evidenceJson, evidence);
    return;
  }
  throw new Error(`signed artifact smoke is not implemented for platform: ${platform}`);
}

function smokeMacArtifacts(releaseRoot, { skipSignature = false } = {}) {
  const appPath = valueAfter("--app") || findMacAppBundle(releaseRoot);
  const infoPlistPath = path.join(appPath, "Contents", "Info.plist");
  const asarPath = path.join(appPath, "Contents", "Resources", "app.asar");
  const helperPath = path.join(appPath, "Contents", "Resources", "ctox", "ctox");
  assert.ok(fs.existsSync(appPath), `macOS app bundle is missing: ${appPath}`);
  assertFile(infoPlistPath, "macOS Info.plist is missing");
  assertFile(asarPath, "macOS app.asar is missing");
  assertExecutable(helperPath, "macOS bundled CTOX helper is missing");
  if (!skipSignature) {
    execFileSync("codesign", ["--verify", "--deep", "--strict", "--verbose=2", appPath], { stdio: "pipe" });
    execFileSync("spctl", ["--assess", "--type", "execute", "--verbose=2", appPath], { stdio: "pipe" });
  }
  console.log(`desktop signed artifact smoke OK (mac): ${appPath}`);
  return createEvidence(releaseRoot, "mac", {
    appBundle: directoryEvidence(releaseRoot, appPath),
    infoPlist: fileEvidence(releaseRoot, infoPlistPath),
    appAsar: fileEvidence(releaseRoot, asarPath),
    bundledHelper: executableEvidence(releaseRoot, helperPath),
    signature: {
      skipped: skipSignature,
      checks: skipSignature ? [] : ["codesign --verify --deep --strict", "spctl --assess --type execute"],
    },
  });
}

function smokeLinuxArtifacts(releaseRoot) {
  const appImage = findFirstFile(releaseRoot, (filePath) => filePath.endsWith(".AppImage"));
  const deb = findFirstFile(releaseRoot, (filePath) => filePath.endsWith(".deb"));
  const unpacked = findFirstDirectory(releaseRoot, (dirPath) => path.basename(dirPath) === "linux-unpacked");
  const asarPath = path.join(unpacked, "resources", "app.asar");
  const helperPath = path.join(unpacked, "resources", "ctox", "ctox");
  assertFile(appImage, "Linux AppImage artifact is missing");
  assertFile(deb, "Linux deb artifact is missing");
  assertDirectory(unpacked, "Linux unpacked app directory is missing");
  assertFile(asarPath, "Linux app.asar is missing");
  assertExecutable(helperPath, "Linux bundled CTOX helper is missing");
  console.log(`desktop signed artifact smoke OK (linux): ${appImage}`);
  return createEvidence(releaseRoot, "linux", {
    appImage: fileEvidence(releaseRoot, appImage),
    deb: fileEvidence(releaseRoot, deb),
    unpackedApp: directoryEvidence(releaseRoot, unpacked),
    appAsar: fileEvidence(releaseRoot, asarPath),
    bundledHelper: executableEvidence(releaseRoot, helperPath),
  });
}

function smokeWindowsArtifacts(releaseRoot) {
  const installer = findFirstFile(
    releaseRoot,
    (filePath) => filePath.endsWith(".exe") && !filePath.includes(`${path.sep}win-unpacked${path.sep}`),
  );
  const unpacked = findFirstDirectory(releaseRoot, (dirPath) => path.basename(dirPath) === "win-unpacked");
  const asarPath = path.join(unpacked, "resources", "app.asar");
  const helperPath = path.join(unpacked, "resources", "ctox", "ctox.exe");
  assertFile(installer, "Windows installer artifact is missing");
  assertDirectory(unpacked, "Windows unpacked app directory is missing");
  assertFile(asarPath, "Windows app.asar is missing");
  assertFile(helperPath, "Windows bundled CTOX helper is missing");
  console.log(`desktop signed artifact smoke OK (win): ${installer}`);
  return createEvidence(releaseRoot, "win", {
    nsisInstaller: fileEvidence(releaseRoot, installer),
    unpackedApp: directoryEvidence(releaseRoot, unpacked),
    appAsar: fileEvidence(releaseRoot, asarPath),
    bundledHelper: fileEvidence(releaseRoot, helperPath),
  });
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
  return findFirstDirectory(releaseRoot, (dirPath) => dirPath.endsWith(".app")) || candidates[0];
}

function findFirstFile(root, predicate) {
  return findFirst(root, (filePath, entry) => entry.isFile() && predicate(filePath));
}

function findFirstDirectory(root, predicate) {
  return findFirst(root, (filePath, entry) => entry.isDirectory() && predicate(filePath));
}

function findFirst(root, predicate) {
  if (!root || !fs.existsSync(root)) return "";
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const fullPath = path.join(root, entry.name);
    if (predicate(fullPath, entry)) return fullPath;
    if (entry.isDirectory()) {
      const nested = findFirst(fullPath, predicate);
      if (nested) return nested;
    }
  }
  return "";
}

function assertFile(filePath, message) {
  assert.ok(filePath && fs.existsSync(filePath), `${message}: ${filePath || "<none>"}`);
  const stat = fs.statSync(filePath);
  assert.ok(stat.isFile(), `${message}: not a file: ${filePath}`);
  assert.ok(stat.size > 0, `${message}: empty file: ${filePath}`);
}

function assertDirectory(dirPath, message) {
  assert.ok(dirPath && fs.existsSync(dirPath), `${message}: ${dirPath || "<none>"}`);
  assert.ok(fs.statSync(dirPath).isDirectory(), `${message}: not a directory: ${dirPath}`);
}

function assertExecutable(filePath, message) {
  assertFile(filePath, message);
  if (process.platform === "win32" || filePath.endsWith(".exe")) return;
  fs.accessSync(filePath, fs.constants.X_OK);
}

function createEvidence(releaseRoot, platform, checks) {
  return {
    schema: "ctox-business-os-desktop-release-artifact-smoke/v1",
    ok: true,
    platform,
    releaseRoot: ".",
    generatedAt: new Date().toISOString(),
    checks,
  };
}

function fileEvidence(releaseRoot, filePath) {
  const stat = fs.statSync(filePath);
  return {
    path: relativeEvidencePath(releaseRoot, filePath),
    sizeBytes: stat.size,
  };
}

function executableEvidence(releaseRoot, filePath) {
  return {
    ...fileEvidence(releaseRoot, filePath),
    executable: isExecutable(filePath),
  };
}

function directoryEvidence(releaseRoot, dirPath) {
  return {
    path: relativeEvidencePath(releaseRoot, dirPath),
  };
}

function relativeEvidencePath(releaseRoot, filePath) {
  return path.relative(path.resolve(releaseRoot), path.resolve(filePath)).split(path.sep).join("/");
}

function isExecutable(filePath) {
  if (process.platform === "win32" || filePath.endsWith(".exe")) return true;
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function writeEvidence(evidenceJson, evidence) {
  if (!evidenceJson) return;
  const outputPath = path.resolve(evidenceJson);
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
}

function normalizePlatform(value) {
  const platform = String(value || "").trim().toLowerCase();
  if (platform === "darwin" || platform === "macos") return "mac";
  if (platform === "win32" || platform === "windows") return "win";
  return platform;
}

function valueAfter(flag) {
  const index = process.argv.indexOf(flag);
  if (index < 0) return "";
  return process.argv[index + 1] || "";
}

function hasFlag(flag) {
  return process.argv.includes(flag);
}

main();
