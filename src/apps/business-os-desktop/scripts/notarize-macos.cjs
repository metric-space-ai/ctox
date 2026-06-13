"use strict";

const { notarize } = require("@electron/notarize");

async function notarizeMacos(context) {
  if (context?.electronPlatformName !== "darwin") return;
  if (isDirOnlyPack(context) || isDirectoryPackRequested()) return;
  const appPath = context?.appOutDir && context?.packager?.appInfo?.productFilename
    ? `${context.appOutDir}/${context.packager.appInfo.productFilename}.app`
    : "";
  if (!appPath) throw new Error("notarization context is missing app path");

  const appleId = process.env.APPLE_ID;
  const appleIdPassword = process.env.APPLE_ID_PASSWORD;
  const teamId = process.env.APPLE_TEAM_ID;
  if (!appleId || !appleIdPassword || !teamId) {
    throw new Error("macOS notarization requires APPLE_ID, APPLE_ID_PASSWORD and APPLE_TEAM_ID build secrets");
  }

  await notarize({
    tool: "notarytool",
    appBundleId: "ai.metric-space.ctox.business-os-desktop",
    appPath,
    appleId,
    appleIdPassword,
    teamId,
  });
}

function isDirOnlyPack(context) {
  const targets = Array.isArray(context?.targets) ? context.targets : [];
  return targets.length > 0 && targets.every((target) => target?.name === "dir");
}

function isDirectoryPackRequested(argv = process.argv) {
  return argv.some((arg) => arg === "--dir" || arg === "dir" || String(arg).endsWith("=dir"));
}

module.exports = notarizeMacos;
module.exports.isDirectoryPackRequested = isDirectoryPackRequested;
module.exports.isDirOnlyPack = isDirOnlyPack;
