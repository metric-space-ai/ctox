"use strict";

const fs = require("node:fs");
const path = require("node:path");

const UPDATE_FEED_URL = "https://ctox.dev/downloads/business-os-desktop";
const ctoxHelperResourceDir = path.join(__dirname, "resources", "ctox");
const extraResources = fs.existsSync(ctoxHelperResourceDir)
  ? [{ from: "resources/ctox", to: "ctox" }]
  : [];

module.exports = {
  appId: "ai.metric-space.ctox.business-os-desktop",
  productName: "CTOX Business-OS Desktop Beta",
  artifactName: "${productName}-${version}-${os}-${arch}.${ext}",
  asar: true,
  directories: {
    output: "release",
    buildResources: "build",
  },
  icon: "build/icon.png",
  files: [
    "package.json",
    "src/**/*",
    "!test/**",
    "!release/**",
    "!node_modules/.cache/**",
    "!**/*.map",
    "!**/.DS_Store",
  ],
  ...(extraResources.length ? { extraResources } : {}),
  protocols: [{
    name: "CTOX Business OS Desktop Beta Pairing",
    schemes: ["ctox-business-os-desktop"],
  }],
  publish: [{
    provider: "generic",
    url: UPDATE_FEED_URL,
  }],
  generateUpdatesFilesForAllChannels: true,
  mac: {
    category: "public.app-category.business",
    hardenedRuntime: true,
    gatekeeperAssess: false,
    icon: "build/icon.icns",
    entitlements: "build/entitlements.mac.plist",
    entitlementsInherit: "build/entitlements.mac.plist",
    target: ["dmg", "zip"],
  },
  afterSign: "scripts/notarize-macos.cjs",
  dmg: {
    sign: true,
  },
  win: {
    target: [{
      target: "nsis",
      arch: ["x64", "arm64"],
    }],
  },
  nsis: {
    oneClick: false,
    perMachine: false,
    allowToChangeInstallationDirectory: true,
    createDesktopShortcut: true,
    createStartMenuShortcut: true,
  },
  linux: {
    category: "Office",
    icon: "build/icon.png",
    maintainer: "CTOX <oss@ctox.dev>",
    target: ["AppImage", "deb"],
  },
};
