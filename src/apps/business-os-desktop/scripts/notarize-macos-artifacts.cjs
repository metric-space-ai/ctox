"use strict";

const path = require("node:path");
const { notarize } = require("@electron/notarize");

async function notarizeMacosArtifacts(buildResult) {
  if (process.platform !== "darwin") return;
  const dmgPaths = findDmgArtifacts(buildResult?.artifactPaths);
  if (dmgPaths.length === 0) return;

  const appleId = process.env.APPLE_ID;
  const appleIdPassword = process.env.APPLE_APP_SPECIFIC_PASSWORD || process.env.APPLE_ID_PASSWORD;
  const teamId = process.env.APPLE_TEAM_ID;
  if (!appleId || !appleIdPassword || !teamId) {
    throw new Error("macOS DMG notarization requires APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD and APPLE_TEAM_ID build secrets");
  }

  for (const appPath of dmgPaths) {
    await notarize({
      tool: "notarytool",
      appBundleId: "ai.metric-space.ctox.business-os-desktop",
      appPath,
      appleId,
      appleIdPassword,
      teamId,
    });
  }
}

function findDmgArtifacts(artifactPaths = []) {
  return artifactPaths.filter((artifactPath) => path.extname(artifactPath).toLowerCase() === ".dmg");
}

module.exports = notarizeMacosArtifacts;
module.exports.findDmgArtifacts = findDmgArtifacts;
