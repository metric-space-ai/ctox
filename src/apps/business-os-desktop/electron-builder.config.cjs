"use strict";

const UPDATE_FEED_URL = "https://ctox.dev/downloads/business-os-desktop";

module.exports = {
  appId: "ai.metric-space.ctox.business-os-desktop",
  productName: "CTOX Business-OS Desktop",
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
  protocols: [{
    name: "CTOX Business OS Desktop Pairing",
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
    target: ["AppImage", "deb"],
  },
};
