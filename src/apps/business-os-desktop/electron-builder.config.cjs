"use strict";

const fs = require("node:fs");
const path = require("node:path");

const UPDATE_FEED_URL = "https://ctox.dev/downloads/business-os-desktop";
const windowsStoreRelease = process.env.CTOX_WINDOWS_STORE_RELEASE === "1";
const ctoxHelperResourceDir = path.join(__dirname, "resources", "ctox");
const extraResources = [
  {
    from: "../business-os",
    to: "business-os",
    filter: ["**/*", "!**/node_modules/**", "!**/.DS_Store"],
  },
  ...(fs.existsSync(ctoxHelperResourceDir) ? [{ from: "resources/ctox", to: "ctox" }] : []),
  {
    from: "../../../install.sh",
    to: "ctox/install.sh",
  },
];

module.exports = {
  appId: "ai.metric-space.ctox.business-os-desktop",
  productName: "CTOX Business-OS Desktop Beta",
  executableName: "ctox-business-os-desktop",
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
  extraResources,
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
    // The afterSign hook below is the single owner of notarization. Electron
    // Builder's built-in pass runs before afterSign and would submit twice.
    notarize: false,
    signIgnore: [
      "/Contents/Resources/business-os/",
      "/Contents/Frameworks/Electron Framework\\.framework/.+\\.lproj/locale\\.pak$",
    ],
    binaries: ["Contents/Resources/ctox/ctox"],
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
    target: windowsStoreRelease
      ? [{ target: "appx", arch: ["x64"] }]
      : [{ target: "nsis", arch: ["x64", "arm64"] }],
  },
  appx: {
    applicationId: "CTOXBusinessOSDesktop",
    identityName: "MichaelWelsch.ctox",
    publisher: "CN=A8C36C19-A31B-4FA0-8621-2C0AB781EA66",
    publisherDisplayName: "Michael Welsch",
    displayName: "CTOX Business OS Desktop",
    languages: ["de-DE", "en-US"],
    showNameOnTiles: true,
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
    syncDesktopName: true,
    target: ["AppImage", "deb"],
  },
};
